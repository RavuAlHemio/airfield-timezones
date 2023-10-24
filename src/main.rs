mod pdf_reading;
mod regex_ext;


use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use clap::Parser;
use once_cell::sync::Lazy;
use pdf::content::{Op, TextDrawAdjusted};
use pdf::file::FileOptions as PdfFileOptions;
use pdf::font::Font;
use pdf::object::MaybeRef;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::pdf_reading::{
    Coords, bookmark_destination_to_page_index, font_decode, get_destination_pages, get_page_references,
    get_top_level_bookmarks, Matrix2D, NoNonsenseF32,
};
use crate::regex_ext::SerializableRegex;


#[cfg(feature = "parsing_hacks")]
static ICAO_AND_UTC: Lazy<Regex> = Lazy::new(|| Regex::new(concat!(
    "\\(",
        "(?P<icao>",
            "[A-Z0-9]{4}",
        ")",
    "\\)",
    ".+",
    "UTC",
    "[ ]?",
    "(?P<utc>",
        "[-+\u{2013}]",
        "[0-9]+",
    ")",
    "(?:",
        "[ ]?",
        "\\(",
            "(?:",
                "(?P<utcdst>", // standard
                    "[-+\u{2013} ]?",
                    "[0-9]+",
                ")",
                "|",
                "(?P<dstutc>", // aberration
                    "[0-9]+",
                    "[-+\u{2013}]",
                ")",
            ")",
            "(?:DT|D|T)?",
        "\\)",
    ")?",
)).unwrap());

#[cfg(not(feature = "parsing_hacks"))]
static ICAO_AND_UTC: Lazy<Regex> = Lazy::new(|| Regex::new(concat!(
    "\\(",
        "(?P<icao>",
            "[A-Z0-9]{4}",
        ")",
    "\\)",
    ".+",
    "UTC",
    "(?P<utc>",
        "[-+\u{2013}]",
        "[0-9]+",
    ")",
    "(?:",
        "\\(",
            "(?:",
                "(?P<utcdst>",
                    "[-+\u{2013}]",
                    "[0-9]+",
                ")",
            ")",
            "DT",
        "\\)",
    ")?",
)).unwrap());


#[derive(Parser)]
struct Opts {
    #[arg(short, long, default_value = "time_zones.toml")]
    pub time_zones: PathBuf,

    pub pdf_paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, Default, Deserialize, Hash, Eq, Ord, PartialEq, PartialOrd, Serialize)]
struct TimeZoneDefinition {
    pub icao_match: Option<SerializableRegex>,
    pub iana: String,
    pub utc_standard: i8,
    pub utc_daylight: Option<i8>,
}


fn normalize_offset(offset: &str) -> i8 {
    let mut mod_offset = offset.replace('\u{2013}', "-");
    if mod_offset.starts_with('+') || mod_offset.starts_with(' ') {
        mod_offset.remove(0);
    }
    mod_offset.parse().unwrap()
}
fn normalize_reverse_offset(offset: &str) -> i8 {
    // "12-" -> "-12"
    let offset_chars: Vec<char> = offset.chars().collect();
    let mut regular_offset = String::with_capacity(offset.len());
    for c in &offset_chars[offset_chars.len()-1..] {
        regular_offset.push(*c);
    }
    for c in &offset_chars[0..offset_chars.len()-1] {
        regular_offset.push(*c);
    }
    normalize_offset(&regular_offset)
}


fn main() {
    let opts = Opts::parse();

    let name_to_timezone: HashMap<String, TimeZoneDefinition> = {
        let time_zones = std::fs::read_to_string(&opts.time_zones)
            .expect("failed to read time zone file");
        toml::from_str(&time_zones)
            .expect("failed to parse time zone file")
    };

    for pdf_path in &opts.pdf_paths {
        let pdf_file = PdfFileOptions::cached()
            .open(pdf_path).expect("failed to open PDF file");
        let top_level_bookmarks = get_top_level_bookmarks(pdf_file.get_root(), &pdf_file);
        let destination_pages = get_destination_pages(pdf_file.get_root(), &pdf_file);
        let page_references = get_page_references(pdf_file.get_root(), &pdf_file);

        let airport_directory_bookmark = top_level_bookmarks.iter()
            .filter(|bkmk| bkmk.title.ends_with(": AIRPORT/FACILITY DIRECTORY"))
            .nth(0).expect("no airport directory bookmark found");
        let airport_directory_page = bookmark_destination_to_page_index(
            &airport_directory_bookmark.destination,
            &destination_pages,
            &page_references,
        )
            .expect("airport directory page not found");

        let bookmark_after_directory_opt = top_level_bookmarks
            .get(airport_directory_bookmark.index + 1);
        let page_after_directory = match bookmark_after_directory_opt {
            Some(bad) => {
                bookmark_destination_to_page_index(
                    &bad.destination,
                    &destination_pages,
                    &page_references,
                )
                    .expect("page for bookmark after airport directory not found")
            },
            None => {
                // airport directory is the last page
                pdf_file.num_pages()
            },
        };

        // run through those pages
        for page_index in airport_directory_page..page_after_directory {
            let page = pdf_file.get_page(page_index)
                .expect("failed to obtain page");
            let Some(contents) = page.contents.as_ref() else { continue };
            let ops = contents.operations(&pdf_file)
                .expect("failed to obtain page ops");

            let fonts: HashMap<&str, &MaybeRef<Font>> = page
                .resources().expect("page has no resources")
                .fonts().collect();

            let mut coordinates_to_text = BTreeMap::new();
            let mut text_matrix = None;
            let mut current_font = None;
            for op in ops {
                match op {
                    Op::BeginText => {
                        text_matrix = Some(Matrix2D::default());
                    },
                    Op::EndText => {
                        text_matrix = None;
                    },
                    Op::SetTextMatrix { matrix } => {
                        text_matrix = Some(Matrix2D {
                            a0: matrix.a.try_into().unwrap(),
                            a1: matrix.b.try_into().unwrap(),
                            a2: NoNonsenseF32::zero(),

                            b0: matrix.c.try_into().unwrap(),
                            b1: matrix.d.try_into().unwrap(),
                            b2: NoNonsenseF32::zero(),

                            c0: matrix.e.try_into().unwrap(),
                            c1: matrix.f.try_into().unwrap(),
                            c2: NoNonsenseF32::one(),
                        });
                    },
                    Op::TextDraw { text } => {
                        let Some(matrix) = &text_matrix else { continue };
                        let mut coords = matrix.apply_to_vector(Coords::default());
                        coords.y = (-f32::from(coords.y)).try_into().unwrap();

                        let Ok(text_string) = text.to_string() else { continue };
                        coordinates_to_text
                            .entry(coords)
                            .or_insert_with(|| String::new())
                            .push_str(&text_string);
                    },
                    Op::TextFont { name, .. } => {
                        current_font = Some(
                            *fonts
                                .get(name.as_str()).expect("unknown font")
                        );
                    },
                    Op::TextDrawAdjusted { array } => {
                        let Some(matrix) = &text_matrix else { continue };
                        let mut coords = matrix.apply_to_vector(Coords::default());
                        coords.y = (-f32::from(coords.y)).try_into().unwrap();

                        for adjustment in array {
                            match adjustment {
                                TextDrawAdjusted::Spacing(_spacing) => {},
                                TextDrawAdjusted::Text(text) => {
                                    let Some(text_string) = font_decode(current_font, text, &pdf_file) else { continue };
                                    coordinates_to_text
                                        .entry(coords)
                                        .or_insert_with(|| String::new())
                                        .push_str(&text_string);
                                },
                            }
                        }
                    },
                    _other => {
                        // println!("{:?}", other);
                    },
                }
            }

            // assemble lines
            let mut lines = BTreeMap::new();
            for (coordinates, text) in &coordinates_to_text {
                let line = lines
                    .entry(coordinates.y)
                    .or_insert_with(|| String::new());
                line.push_str(text);
            }
            for line in lines.values() {
                if let Some(caps) = ICAO_AND_UTC.captures(line) {
                    let icao = caps.name("icao").expect("did not capture icao").as_str();
                    let offset = normalize_offset(caps.name("utc").expect("did not capture utc").as_str());
                    let dst_offset = caps.name("utcdst")
                        .map(|d| normalize_offset(d.as_str()))
                        // handle typographical error "UTC-5( 4DT)"
                        .map(|doff| if offset < -2 && doff > 2 { -doff } else { doff })
                        .or_else(|| caps.name("dstutc").map(|d| normalize_reverse_offset(d.as_str())));

                    // match timezone
                    let mut iana_timezone_opt = None;
                    for timezone in name_to_timezone.values() {
                        if let Some(icao_match) = timezone.icao_match.as_ref() {
                            if !icao_match.0.is_match(icao) {
                                continue;
                            }
                        }
                        if offset == timezone.utc_standard && dst_offset == timezone.utc_daylight {
                            iana_timezone_opt = Some(timezone.iana.clone());
                            break;
                        }
                    }

                    if let Some(iana_timezone) = iana_timezone_opt.as_ref() {
                        println!("{} {}", icao, iana_timezone);
                    } else {
                        println!("{} ?", icao);
                    }
                }
            }
        }
    }
}
