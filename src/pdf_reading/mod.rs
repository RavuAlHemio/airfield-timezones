mod encoding;


use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use pdf::encoding::BaseEncoding;
use pdf::font::Font;
use pdf::object::{
    Action, Catalog, MaybeNamedDest, MaybeRef, Page, PagesNode, PageTree, Ref, Resolve,
};
use pdf::primitive::PdfString;

use crate::pdf_reading::encoding::{
    MAC_ROMAN_ENCODING, NAME_TO_CHARACTER, STANDARD_ENCODING, SYMBOL_ENCODING, WIN_ANSI_ENCODING,
};


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum BookmarkDestination {
    Named(String),
    Page(Ref<Page>),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct Bookmark {
    pub index: usize,
    pub title: String,
    pub destination: BookmarkDestination,
}


#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct NoNonsenseF32(f32);
impl NoNonsenseF32 {
    pub const fn zero() -> Self { Self(0.0) }
    pub const fn one() -> Self { Self(1.0) }
}
impl TryFrom<f32> for NoNonsenseF32 {
    type Error = f32;

    fn try_from(value: f32) -> Result<Self, Self::Error> {
        if value.is_finite() {
            Ok(Self(value))
        } else {
            Err(value)
        }
    }
}
impl From<NoNonsenseF32> for f32 {
    fn from(value: NoNonsenseF32) -> Self { value.0 }
}
impl PartialEq for NoNonsenseF32 {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl Eq for NoNonsenseF32 {}
impl PartialOrd for NoNonsenseF32 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.0.partial_cmp(&other.0).unwrap())
    }
}
impl Ord for NoNonsenseF32 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}
impl Hash for NoNonsenseF32 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}


#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct Coords {
    pub y: NoNonsenseF32,
    pub x: NoNonsenseF32,
}


#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct Matrix2D {
    pub a0: NoNonsenseF32,
    pub b0: NoNonsenseF32,
    pub c0: NoNonsenseF32,
    pub a1: NoNonsenseF32,
    pub b1: NoNonsenseF32,
    pub c1: NoNonsenseF32,
    pub a2: NoNonsenseF32,
    pub b2: NoNonsenseF32,
    pub c2: NoNonsenseF32,
}
impl Matrix2D {
    pub fn apply_to_vector(&self, vector: Coords) -> Coords {
        //           ⎡x⎤
        //           ⎢y⎥
        //           ⎣1⎦
        // ⎡a0 b0 c0⎤⎡nx ⎤
        // ⎢a1 b1 c1⎥⎢ny ⎥
        // ⎣a2 b2 c2⎦⎣nvm⎦

        let nx =
            f32::from(self.a0) * f32::from(vector.x)
            + f32::from(self.b0) * f32::from(vector.y)
            + f32::from(self.c0)
        ;
        let ny =
            f32::from(self.a1) * f32::from(vector.x)
            + f32::from(self.b1) * f32::from(vector.y)
            + f32::from(self.c1)
        ;
        Coords {
            x: nx.try_into().unwrap(),
            y: ny.try_into().unwrap(),
        }
    }
}
impl Default for Matrix2D {
    fn default() -> Self {
        Self {
            a0: NoNonsenseF32::one(),
            b0: NoNonsenseF32::zero(),
            c0: NoNonsenseF32::zero(),

            a1: NoNonsenseF32::zero(),
            b1: NoNonsenseF32::one(),
            c1: NoNonsenseF32::zero(),

            a2: NoNonsenseF32::zero(),
            b2: NoNonsenseF32::zero(),
            c2: NoNonsenseF32::one(),
        }
    }
}


pub(crate) fn get_top_level_bookmarks<R: Resolve>(pdf_root: &Catalog, resolve: &R) -> Vec<Bookmark> {
    let Some(outlines) = pdf_root.outlines.as_ref() else { return Vec::with_capacity(0) };
    let Some(first_outline_ref) = outlines.first else { return Vec::with_capacity(0) };
    let mut current_outline = resolve.get(first_outline_ref)
        .expect("failed to resolve first outline");
    let mut ret = Vec::new();
    loop {
        let title_opt = current_outline.title
            .as_ref()
            .map(|t| t.to_string().expect("failed to decode string"));
        if let Some(title) = title_opt {
            let bookmark_opt = if let Some(dest) = current_outline.dest.as_ref() {
                let dest_string = dest
                    .as_string().expect("destination not a string")
                    .to_string().expect("failed to decode string");
                Some(Bookmark {
                    index: ret.len(),
                    title,
                    destination: BookmarkDestination::Named(dest_string),
                })
            } else if let Some(action) = current_outline.action.as_ref() {
                match action {
                    Action::Goto(goto) => Some(goto),
                    other => {
                        println!("alternative action for {:?}: {:?}", title, other);
                        None
                    },
                }
                    .and_then(|mnd| match mnd {
                        MaybeNamedDest::Direct(dest) => {
                            dest.page.map(|page| Bookmark {
                                index: ret.len(),
                                title,
                                destination: BookmarkDestination::Page(page),
                            })
                        },
                        MaybeNamedDest::Named(nd) => {
                            let destination_text = nd.to_string()
                                .expect("failed to decode destination string");
                            Some(Bookmark {
                                index: ret.len(),
                                title,
                                destination: BookmarkDestination::Named(destination_text),
                            })
                        },
                    })
            } else {
                None
            };
            if let Some(bookmark) = bookmark_opt {
                ret.push(bookmark);
            }
        }

        let next_outline_ref = match current_outline.next {
            Some(n) => n,
            None => break,
        };
        current_outline = resolve.get(next_outline_ref)
            .expect("failed to resolve next outline");
    }
    ret
}


pub(crate) fn collect_page_references<R: Resolve>(page_tree: &PageTree, resolve: &R, page_refs: &mut Vec<Ref<Page>>, depth: usize) {
    if depth == 0 {
        panic!("in too deep");
    }

    for kid_ref in &page_tree.kids {
        let kid = resolve.get(*kid_ref)
            .expect("failed to resolve page node");
        match &*kid {
            PagesNode::Tree(kid_tree) => {
                collect_page_references(kid_tree, resolve, page_refs, depth -1);
            },
            PagesNode::Leaf(_) => {
                // only store the reference
                page_refs.push(Ref::new(kid_ref.get_inner()));
            },
        }
    }
}

pub(crate) fn get_page_references<R: Resolve>(pdf_root: &Catalog, resolve: &R) -> Vec<Ref<Page>> {
    let mut page_refs = Vec::new();
    collect_page_references(&pdf_root.pages, resolve, &mut page_refs, 16);
    page_refs
}


pub(crate) fn get_destination_pages<R: Resolve>(pdf_root: &Catalog, resolve: &R) -> HashMap<String, u32> {
    let page_refs = get_page_references(pdf_root, resolve);
    let Some(names_ref) = pdf_root.names.as_ref() else { return HashMap::with_capacity(0) };
    let names = names_ref.data();
    let Some(dests) = names.dests.as_ref() else { return HashMap::with_capacity(0) };
    let mut ret = HashMap::new();
    dests.walk(resolve, &mut |name, dest_opt| {
        let Ok(dest_name) = name.to_string() else { return };
        let Some(dest) = dest_opt else { return };
        let Some(page_ref) = dest.page else { return };
        let page_index = page_refs.iter().position(|pr| pr == &page_ref);
        if let Some(pi) = page_index {
            if let Ok(pi32) = u32::try_from(pi) {
                ret.insert(dest_name, pi32);
            }
        }
    })
        .expect("failed to walk dests");
    ret
}


pub(crate) fn bookmark_destination_to_page_index(
    destination: &BookmarkDestination,
    destination_pages: &HashMap<String, u32>,
    page_references: &[Ref<Page>],
) -> Option<u32> {
    match destination {
        BookmarkDestination::Named(name) => {
            destination_pages.get(name).map(|v| *v)
        },
        BookmarkDestination::Page(page_ref) => {
            page_references.iter()
                .position(|pr| pr == page_ref)
                .map(|i| u32::try_from(i).unwrap())
        },
    }
}


pub(crate) fn font_decode<R: Resolve>(current_font_opt: Option<&MaybeRef<Font>>, text: PdfString, resolve: &R) -> Option<String> {
    let Some(current_font) = current_font_opt else { return None };
    let text_bytes = text.as_bytes();
    if let Some(itu) = current_font.to_unicode(resolve) {
        // direct to-Unicode map
        let index_to_unicode = itu.expect("failed to obtain Unicode map");
        let mut ret = String::with_capacity(text_bytes.len() / 2);
        for i in (0..text_bytes.len()).step_by(2) {
            let index = u16::from_be_bytes(text_bytes[i..i+2].try_into().unwrap());
            let unicode = index_to_unicode.get(index)
                .expect("unexpected index");
            ret.push_str(unicode);
        }
        Some(ret)
    } else if let Some(encoding) = current_font.encoding() {
        // use encoding
        let mut encoding_map = match encoding.base {
            BaseEncoding::StandardEncoding => STANDARD_ENCODING.clone(),
            BaseEncoding::SymbolEncoding => SYMBOL_ENCODING.clone(),
            BaseEncoding::MacRomanEncoding => MAC_ROMAN_ENCODING.clone(),
            BaseEncoding::WinAnsiEncoding => WIN_ANSI_ENCODING.clone(),
            BaseEncoding::MacExpertEncoding => return None,
            BaseEncoding::IdentityH => return None,
            BaseEncoding::None => return None,
            BaseEncoding::Other(_) => return None,
        };
        for (byte, char_name) in &encoding.differences {
            let byte_u8: u8 = (*byte).try_into().unwrap();
            let char_name_str = char_name.as_str();
            if let Some(char_value) = NAME_TO_CHARACTER.get(char_name_str) {
                encoding_map.insert(byte_u8, *char_value);
            }
        }

        // decode
        let mut ret = String::with_capacity(text_bytes.len());
        for b in text_bytes {
            if let Some(c) = encoding_map.get(b) {
                ret.push(*c);
            }
        }
        Some(ret)
    } else {
        None
    }
}
