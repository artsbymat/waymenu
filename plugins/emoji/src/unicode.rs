use waymenu_plugin_api::{PluginResult, AUTOCOMPLETE_METADATA_PREFIX};

use super::{ranked_entries, Entry, MATCH_LIMIT};

pub(super) fn codepoint_query(query: &str) -> Option<&str> {
    query
        .strip_prefix("U+")
        .or_else(|| query.strip_prefix("u+"))
}

pub(super) fn search(query: &str) -> Vec<PluginResult> {
    if let Some(hex) = codepoint_query(query) {
        return lookup_codepoint(hex).into_iter().collect();
    }
    let limit = if query.is_empty() {
        SYMBOLS.len()
    } else {
        MATCH_LIMIT
    };
    ranked_entries(SYMBOLS.iter().copied(), query, true, limit)
        .into_iter()
        .map(|(score, (glyph, name, _, category))| result(glyph, name, category, score))
        .collect()
}

fn lookup_codepoint(hex: &str) -> Option<PluginResult> {
    if hex.len() > 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(hex, 16).ok()?;
    let character = char::from_u32(value)?;
    if character.is_control() || (0xFDD0..=0xFDEF).contains(&value) || value & 0xFFFF >= 0xFFFE {
        return None;
    }
    let mut bytes = [0; 4];
    let glyph = character.encode_utf8(&mut bytes);
    let (name, category) = SYMBOLS
        .iter()
        .find(|entry| entry.0 == glyph)
        .map_or(("Unicode character", "Unicode"), |entry| (entry.1, entry.3));
    Some(result(glyph, name, category, 10_000))
}

fn result(glyph: &str, name: &str, category: &str, score: i64) -> PluginResult {
    let codepoint = format!("U+{:04X}", u32::from(glyph.chars().next().unwrap()));
    PluginResult {
        id: glyph.to_owned(),
        title: format!("{glyph}  {name}"),
        description: format!("{codepoint} · {category}"),
        score,
        icon: None,
        image: None,
        metadata: Some(format!("{AUTOCOMPLETE_METADATA_PREFIX}unicode {codepoint}")),
    }
}

const ARROWS: &str = "Arrows";
const MATH: &str = "Mathematics";
const CURRENCY: &str = "Currency";
const PUNCTUATION: &str = "Punctuation";
const SHAPES: &str = "Shapes";
const TECHNICAL: &str = "Technical";
const MUSIC: &str = "Music";
const GREEK: &str = "Greek";

static SYMBOLS: &[Entry] = &[
    ("→", "Rightwards Arrow", "right arrow, next", ARROWS),
    ("←", "Leftwards Arrow", "left arrow, back", ARROWS),
    ("↑", "Upwards Arrow", "up arrow", ARROWS),
    ("↓", "Downwards Arrow", "down arrow", ARROWS),
    ("↔", "Left Right Arrow", "horizontal arrow", ARROWS),
    ("↕", "Up Down Arrow", "vertical arrow", ARROWS),
    ("↖", "North West Arrow", "upper left arrow", ARROWS),
    (
        "↗",
        "North East Arrow",
        "upper right arrow, external link",
        ARROWS,
    ),
    ("↘", "South East Arrow", "lower right arrow", ARROWS),
    ("↙", "South West Arrow", "lower left arrow", ARROWS),
    (
        "↵",
        "Downwards Arrow with Corner Leftwards",
        "return, enter",
        ARROWS,
    ),
    ("⇒", "Rightwards Double Arrow", "implies", ARROWS),
    ("⇐", "Leftwards Double Arrow", "implied by", ARROWS),
    ("⇔", "Left Right Double Arrow", "iff, equivalent", ARROWS),
    ("↦", "Rightwards Arrow from Bar", "maps to", ARROWS),
    ("±", "Plus-Minus Sign", "plus minus", MATH),
    ("−", "Minus Sign", "subtract", MATH),
    ("×", "Multiplication Sign", "multiply, times", MATH),
    ("÷", "Division Sign", "divide", MATH),
    ("≠", "Not Equal To", "not equals", MATH),
    ("≤", "Less-Than or Equal To", "less than or equal", MATH),
    (
        "≥",
        "Greater-Than or Equal To",
        "greater than or equal",
        MATH,
    ),
    ("≈", "Almost Equal To", "approximately, approx", MATH),
    ("≡", "Identical To", "equivalent, congruent", MATH),
    ("∞", "Infinity", "infinite, unbounded", MATH),
    ("√", "Square Root", "radical, sqrt", MATH),
    ("∑", "N-Ary Summation", "sum, sigma", MATH),
    ("∏", "N-Ary Product", "product, pi", MATH),
    ("∫", "Integral", "integration, calculus", MATH),
    ("∂", "Partial Differential", "derivative, calculus", MATH),
    ("∇", "Nabla", "gradient, del", MATH),
    ("∝", "Proportional To", "proportionality", MATH),
    ("∅", "Empty Set", "null set", MATH),
    ("∈", "Element Of", "set membership", MATH),
    ("∉", "Not an Element Of", "set membership", MATH),
    ("⊂", "Subset Of", "proper subset", MATH),
    ("⊆", "Subset of or Equal To", "subset", MATH),
    ("∪", "Union", "set union", MATH),
    ("∩", "Intersection", "set intersection", MATH),
    ("∀", "For All", "universal quantifier", MATH),
    ("∃", "There Exists", "existential quantifier", MATH),
    ("½", "Vulgar Fraction One Half", "fraction, 1/2", MATH),
    ("¼", "Vulgar Fraction One Quarter", "fraction, 1/4", MATH),
    ("¾", "Vulgar Fraction Three Quarters", "fraction, 3/4", MATH),
    ("²", "Superscript Two", "squared, power", MATH),
    ("³", "Superscript Three", "cubed, power", MATH),
    ("€", "Euro Sign", "euros, eur", CURRENCY),
    ("£", "Pound Sign", "sterling, gbp", CURRENCY),
    ("¥", "Yen Sign", "yen, yuan, jpy, cny", CURRENCY),
    ("¢", "Cent Sign", "cents", CURRENCY),
    ("₹", "Indian Rupee Sign", "rupee, inr", CURRENCY),
    ("₩", "Won Sign", "won, krw", CURRENCY),
    ("₽", "Ruble Sign", "ruble, rub", CURRENCY),
    ("₱", "Peso Sign", "peso, php", CURRENCY),
    ("₺", "Turkish Lira Sign", "lira, try", CURRENCY),
    ("฿", "Thai Currency Symbol Baht", "baht, thb", CURRENCY),
    ("₫", "Dong Sign", "dong, vnd", CURRENCY),
    ("₿", "Bitcoin Sign", "bitcoin, btc", CURRENCY),
    (
        "…",
        "Horizontal Ellipsis",
        "three dots, omission",
        PUNCTUATION,
    ),
    ("–", "En Dash", "dash, range", PUNCTUATION),
    ("—", "Em Dash", "dash, long dash", PUNCTUATION),
    (
        "‘",
        "Left Single Quotation Mark",
        "opening single quote",
        PUNCTUATION,
    ),
    (
        "’",
        "Right Single Quotation Mark",
        "apostrophe, closing single quote",
        PUNCTUATION,
    ),
    (
        "“",
        "Left Double Quotation Mark",
        "opening double quote",
        PUNCTUATION,
    ),
    (
        "”",
        "Right Double Quotation Mark",
        "closing double quote",
        PUNCTUATION,
    ),
    (
        "«",
        "Left-Pointing Double Angle Quotation Mark",
        "guillemet",
        PUNCTUATION,
    ),
    (
        "»",
        "Right-Pointing Double Angle Quotation Mark",
        "guillemet",
        PUNCTUATION,
    ),
    ("•", "Bullet", "list, dot", PUNCTUATION),
    ("·", "Middle Dot", "interpunct", PUNCTUATION),
    ("†", "Dagger", "footnote, obelisk", PUNCTUATION),
    ("‡", "Double Dagger", "footnote, diesis", PUNCTUATION),
    ("§", "Section Sign", "section, legal", PUNCTUATION),
    ("¶", "Pilcrow Sign", "paragraph", PUNCTUATION),
    ("‰", "Per Mille Sign", "per thousand, permille", PUNCTUATION),
    (
        "¿",
        "Inverted Question Mark",
        "spanish, question",
        PUNCTUATION,
    ),
    (
        "¡",
        "Inverted Exclamation Mark",
        "spanish, exclamation",
        PUNCTUATION,
    ),
    ("✓", "Check Mark", "tick, correct, done", SHAPES),
    ("✗", "Ballot X", "cross, wrong, cancel", SHAPES),
    ("★", "Black Star", "filled star", SHAPES),
    ("☆", "White Star", "outline star", SHAPES),
    ("●", "Black Circle", "filled circle", SHAPES),
    ("○", "White Circle", "outline circle", SHAPES),
    ("■", "Black Square", "filled square", SHAPES),
    ("□", "White Square", "outline square", SHAPES),
    ("▲", "Black Up-Pointing Triangle", "filled triangle", SHAPES),
    (
        "△",
        "White Up-Pointing Triangle",
        "outline triangle",
        SHAPES,
    ),
    ("◆", "Black Diamond", "filled diamond", SHAPES),
    ("◇", "White Diamond", "outline diamond", SHAPES),
    ("©", "Copyright Sign", "rights, legal", TECHNICAL),
    ("®", "Registered Sign", "registered trademark", TECHNICAL),
    ("™", "Trade Mark Sign", "trademark, brand", TECHNICAL),
    ("°", "Degree Sign", "degrees, temperature, angle", TECHNICAL),
    ("µ", "Micro Sign", "micro, si, mu", TECHNICAL),
    ("№", "Numero Sign", "number, abbreviation", TECHNICAL),
    (
        "⌘",
        "Place of Interest Sign",
        "command, keyboard, mac",
        TECHNICAL,
    ),
    ("⌥", "Option Key", "option, alt, keyboard, mac", TECHNICAL),
    ("⏎", "Return Symbol", "return, enter, keyboard", TECHNICAL),
    ("♩", "Quarter Note", "crotchet", MUSIC),
    ("♪", "Eighth Note", "quaver", MUSIC),
    ("♫", "Beamed Eighth Notes", "notes, quavers", MUSIC),
    ("♭", "Music Flat Sign", "flat, accidental", MUSIC),
    ("♮", "Music Natural Sign", "natural, accidental", MUSIC),
    ("♯", "Music Sharp Sign", "sharp, accidental", MUSIC),
    ("α", "Greek Small Letter Alpha", "alpha", GREEK),
    ("β", "Greek Small Letter Beta", "beta", GREEK),
    ("γ", "Greek Small Letter Gamma", "gamma", GREEK),
    ("δ", "Greek Small Letter Delta", "delta, difference", GREEK),
    ("Δ", "Greek Capital Letter Delta", "delta, change", GREEK),
    ("ε", "Greek Small Letter Epsilon", "epsilon", GREEK),
    ("θ", "Greek Small Letter Theta", "theta, angle", GREEK),
    (
        "λ",
        "Greek Small Letter Lambda",
        "lambda, wavelength",
        GREEK,
    ),
    ("μ", "Greek Small Letter Mu", "mu, mean", GREEK),
    ("π", "Greek Small Letter Pi", "pi", GREEK),
    (
        "σ",
        "Greek Small Letter Sigma",
        "sigma, standard deviation",
        GREEK,
    ),
    ("Σ", "Greek Capital Letter Sigma", "sigma, sum", GREEK),
    ("φ", "Greek Small Letter Phi", "phi, golden ratio", GREEK),
    ("ω", "Greek Small Letter Omega", "omega", GREEK),
    ("Ω", "Greek Capital Letter Omega", "omega, ohm", GREEK),
];
