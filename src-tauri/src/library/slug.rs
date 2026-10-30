//! Suggesting a book id from a folder name.

use super::create::id_taken;
use crate::book_format::validate_book_id;
use std::path::Path;

/// Longest slug before a suffix. `validate_book_id` allows 64; this leaves room for `-book`
/// or a collision counter such as `-12345`.
const MAX_BASE_LEN: usize = 48;

/// Id used when the folder name has no ASCII letters or digits.
const FALLBACK_ID: &str = "book";

/// Lowercase ASCII slug of `name`: letters and digits are kept, each run of any other
/// character becomes one `-`, and separators at either end are dropped.
fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.truncate(MAX_BASE_LEN);
    out.trim_matches(|c| matches!(c, '-' | '.')).to_string()
}

/// The slug of `name`, adjusted so that it passes `validate_book_id` (empty names and
/// Windows device names such as `con` are the only cases that need it).
fn base_id(name: &str) -> String {
    let slug = slugify(name);
    if slug.is_empty() {
        return FALLBACK_ID.to_string();
    }
    if validate_book_id(&slug).is_err() {
        return format!("{slug}-{FALLBACK_ID}");
    }
    slug
}

/// A valid book id for `folder_name` that no entry in `library` already uses.
///
/// The result always passes `validate_book_id`. A taken id (matched case-insensitively, as
/// create does) gets `-2`, `-3`, and so on. A library that cannot be read counts as empty,
/// because create reports that failure itself.
pub fn suggest_book_id(folder_name: &str, library: &Path) -> String {
    let base = base_id(folder_name);
    let mut candidate = base.clone();
    let mut n = 2u32;
    while validate_book_id(&candidate).is_err() || id_taken(library, &candidate).unwrap_or(false) {
        candidate = format!("{base}-{n}");
        n += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::tests::Tmp;

    /// An empty directory to use as a library.
    fn empty_lib(tag: &str) -> (Tmp, std::path::PathBuf) {
        let t = Tmp::new(tag);
        let lib = t.dir("lib");
        (t, lib)
    }

    #[test]
    fn rejected_ids_become_valid_suggestions() {
        // The rejected cases from the book_format tests, plus extras.
        let (_t, lib) = empty_lib("slug-bad");
        for bad in [
            "",
            "../etc",
            "Has Caps",
            ".",
            "bad/id",
            "con",
            "aux",
            "nul",
            "com1",
            "lpt9",
            "con.txt",
            "prn.png",
            "book.",
            "ok\n",
            "-",
            "_x",
            ".hidden",
            "CON.",
            "  ",
            "a/b\\c",
            "nul\0",
            "日本語",
            "COM9",
        ] {
            let id = suggest_book_id(bad, &lib);
            assert!(validate_book_id(&id).is_ok(), "{bad:?} -> {id:?}");
        }
    }

    #[test]
    fn accepted_ids_stay_valid() {
        let (_t, lib) = empty_lib("slug-good");
        for ok in [
            "a",
            "0",
            "ok",
            "a.b_c-d",
            "book-1",
            "v1.2",
            "x".repeat(64).as_str(),
        ] {
            let id = suggest_book_id(ok, &lib);
            assert!(validate_book_id(&id).is_ok(), "{ok:?} -> {id:?}");
        }
        assert_eq!(suggest_book_id("my-book", &lib), "my-book");
    }

    #[test]
    fn slug_rules() {
        let (_t, lib) = empty_lib("slug-rules");
        assert_eq!(suggest_book_id("日本語", &lib), "book");
        assert_eq!(suggest_book_id("--Foo  Bar..", &lib), "foo-bar");
        assert_eq!(suggest_book_id("Moby_Dick (1851)", &lib), "moby-dick-1851");
        assert_eq!(suggest_book_id("", &lib), "book");
        assert_eq!(suggest_book_id("Café Ünï", &lib), "caf-n");
    }

    #[test]
    fn windows_reserved_names_get_a_suffix() {
        let (_t, lib) = empty_lib("slug-reserved");
        for name in ["CON", "nul", "com1", "LPT9", "con.txt"] {
            let id = suggest_book_id(name, &lib);
            assert!(validate_book_id(&id).is_ok(), "{name:?} -> {id:?}");
            assert_ne!(id, name.to_lowercase());
        }
        assert_eq!(suggest_book_id("CON", &lib), "con-book");
    }

    #[test]
    fn long_names_are_truncated_and_stay_valid() {
        let (_t, lib) = empty_lib("slug-long");
        let long = "a".repeat(200);
        let id = suggest_book_id(&long, &lib);
        assert!(validate_book_id(&id).is_ok(), "{id:?}");
        assert_eq!(id.len(), MAX_BASE_LEN);
        std::fs::create_dir(lib.join(&id)).unwrap();
        let next = suggest_book_id(&long, &lib);
        assert!(validate_book_id(&next).is_ok(), "{next:?}");
        assert_eq!(next, format!("{id}-2"));
    }

    #[test]
    fn truncation_does_not_leave_a_trailing_separator() {
        let (_t, lib) = empty_lib("slug-trunc");
        let name = format!("{}  tail", "a".repeat(MAX_BASE_LEN - 1));
        let id = suggest_book_id(&name, &lib);
        assert!(validate_book_id(&id).is_ok(), "{id:?}");
        assert!(!id.ends_with('-'), "{id:?}");
    }

    #[test]
    fn collisions_count_up_and_ignore_case() {
        let (_t, lib) = empty_lib("slug-collide");
        assert_eq!(suggest_book_id("Foo", &lib), "foo");
        std::fs::create_dir(lib.join("foo")).unwrap();
        assert_eq!(suggest_book_id("Foo", &lib), "foo-2");
        std::fs::create_dir(lib.join("FOO-2")).unwrap();
        assert_eq!(suggest_book_id("Foo", &lib), "foo-3");
    }

    #[test]
    fn a_missing_library_counts_as_empty() {
        let (_t, lib) = empty_lib("slug-missing");
        assert_eq!(suggest_book_id("Foo", &lib.join("nope")), "foo");
    }

    #[test]
    fn nasty_inputs_always_validate() {
        let (_t, lib) = empty_lib("slug-nasty");
        std::fs::create_dir(lib.join("book")).unwrap();
        let mut inputs: Vec<String> = [
            "",
            " ",
            ".",
            "..",
            "...",
            "-",
            "--",
            "_",
            "CON",
            "con.",
            "Con.Txt",
            "aux.x.y",
            "LPT1",
            "com0",
            "ａｂｃ",
            "😀😀",
            "tab\tname",
            "new\nline",
            "nul\0byte",
            "a b c",
            "../../etc",
            "C:\\Windows",
            "name.",
            "-.-",
            ".-.",
            "9lives",
            "ÀÉÎ",
            "book",
            "book-2",
        ]
        .map(String::from)
        .into();
        for n in [1, 47, 48, 49, 63, 64, 65, 200, 5000] {
            inputs.push("x".repeat(n));
            inputs.push("-".repeat(n));
            inputs.push(format!("{}.", "a".repeat(n)));
            inputs.push(format!("con{}", ".".repeat(n)));
        }
        for name in &inputs {
            let id = suggest_book_id(name, &lib);
            assert!(validate_book_id(&id).is_ok(), "{name:?} -> {id:?}");
            assert!(!lib.join(&id).exists(), "{name:?} -> {id:?} is taken");
        }
    }
}
