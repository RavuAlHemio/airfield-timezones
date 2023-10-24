use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;


macro_rules! writeln_expect {
    ($target:expr $(, $arg:expr)* $(,)?) => {
        writeln!($target $(, $arg)*).expect("failed to write")
    };
}


fn store_index(character: char, field: &str, map: &mut BTreeMap<u8, char>) {
    if field == "-" {
        return;
    }
    let index = match u8::from_str_radix(field, 8) {
        Ok(i) => i,
        Err(_) => {
            panic!("failed to parse {:?} as octal", field);
        },
    };
    map.insert(index, character);
}


fn main() {
    println!("cargo:rerun-if-changed=src/pdf_reading/encoding.txt");

    let encodings_data = std::fs::read_to_string("src/pdf_reading/encoding.txt")
        .expect("failed to read encodings definition to string");
    let mut index_to_std_char: BTreeMap<u8, char> = BTreeMap::new();
    let mut index_to_mac_char: BTreeMap<u8, char> = BTreeMap::new();
    let mut index_to_win_char: BTreeMap<u8, char> = BTreeMap::new();
    let mut index_to_pdf_char: BTreeMap<u8, char> = BTreeMap::new();
    let mut index_to_sym_char: BTreeMap<u8, char> = BTreeMap::new();
    let mut char_to_name: BTreeMap<char, String> = BTreeMap::new();

    for raw_line in encodings_data.split('\n') {
        let pieces: Vec<&str> = raw_line.trim_end_matches('\r')
            .split('\t')
            .collect();
        if pieces.len() != 7 {
            continue;
        }
        if pieces[0].starts_with("##") {
            continue;
        }

        let character = if let Some(unprefixed) = pieces[0].strip_prefix("U+") {
            let char_value = u32::from_str_radix(unprefixed, 16)
                .expect("failed to parse Unicode codepoint as hex");
            let c = char::from_u32(char_value)
                .expect("hex value does not map to valid character");
            c
        } else if pieces[0].chars().count() != 1 {
            panic!("character {:?} is actually multiple characters", pieces[0]);
        } else {
            pieces[0].chars().nth(0).unwrap()
        };
        char_to_name.insert(character, pieces[1].to_owned());
        store_index(character, pieces[2], &mut index_to_std_char);
        store_index(character, pieces[3], &mut index_to_mac_char);
        store_index(character, pieces[4], &mut index_to_win_char);
        store_index(character, pieces[5], &mut index_to_pdf_char);
        store_index(character, pieces[6], &mut index_to_sym_char);
    }

    let encodings = [
        ("STANDARD_ENCODING", &index_to_std_char),
        ("MAC_ROMAN_ENCODING", &index_to_mac_char),
        ("WIN_ANSI_ENCODING", &index_to_win_char),
        ("PDF_DOC_ENCODING", &index_to_pdf_char),
        ("SYMBOL_ENCODING", &index_to_sym_char),
    ];
    let mut output = File::create("src/pdf_reading/encoding.rs")
        .expect("failed to create output file");
    writeln_expect!(output, "// This file has been automatically generated from encoding.txt.");
    writeln_expect!(output, "// Any changes made manually will be lost.");
    writeln_expect!(output);
    writeln_expect!(output);
    writeln_expect!(output, "use std::collections::HashMap;");
    writeln_expect!(output);
    writeln_expect!(output, "use once_cell::sync::Lazy;");
    writeln_expect!(output);
    for (enc_name, enc_map) in encodings {
        writeln_expect!(output);
        writeln_expect!(output, "pub(crate) static {}: Lazy<HashMap<u8, char>> = Lazy::new(|| {{", enc_name);
        writeln_expect!(output, "    let mut map = HashMap::with_capacity({});", enc_map.len());
        for (byte, character) in enc_map {
            if *character >= ' ' && *character <= '~' {
                writeln_expect!(output, "    map.insert(0o{:o}, {:?});", byte, character);
            } else {
                writeln_expect!(output, "    map.insert(0o{:o}, '\\u{}{:02X}{}');", byte, '{', u32::from(*character), '}');
            }
        }
        writeln_expect!(output, "    map");
        writeln_expect!(output, "}});");
    }
    writeln_expect!(output);
    writeln_expect!(output, "pub(crate) static NAME_TO_CHARACTER: Lazy<HashMap<&'static str, char>> = Lazy::new(|| {{");
    writeln_expect!(output, "    let mut map = HashMap::with_capacity({});", char_to_name.len());
    for (character, name) in &char_to_name {
        if *character >= ' ' && *character <= '~' {
            writeln_expect!(output, "    map.insert({:?}, {:?});", name, character);
        } else {
            writeln_expect!(output, "    map.insert({:?}, '\\u{}{:02X}{}');", name, '{', u32::from(*character), '}');
        }
    }
    writeln_expect!(output, "    map");
    writeln_expect!(output, "}});");
}
