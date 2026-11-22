use waymenu_plugin_api::PluginResult;

use super::row;

pub(super) fn name(code: &str) -> Option<&'static str> {
    LANGUAGES
        .iter()
        .find(|(target, _, _)| target.eq_ignore_ascii_case(code))
        .map(|&(_, name, _)| name)
}

pub(super) fn suggestions(query: &str) -> Vec<PluginResult> {
    let query = query.to_ascii_lowercase();
    let mut matches: Vec<_> = LANGUAGES
        .iter()
        .filter_map(|&(code, name, alias)| {
            let lower_code = code.to_ascii_lowercase();
            let lower_name = name.to_ascii_lowercase();
            let rank = if query == lower_code {
                3
            } else if lower_code.starts_with(&query)
                || lower_name.starts_with(&query)
                || alias.starts_with(&query)
            {
                2
            } else if query.split_whitespace().all(|part| {
                lower_name.contains(part) || alias.contains(part) || lower_code.contains(part)
            }) {
                1
            } else {
                return None;
            };
            Some((rank, code, name))
        })
        .collect();
    matches.sort_by_key(|&(rank, _, _)| std::cmp::Reverse(rank));
    matches
        .into_iter()
        .map(|(_, code, name)| row(&format!("@{code} "), name, &format!("@{code} · Tab to use")))
        .collect()
}

const LANGUAGES: &[(&str, &str, &str)] = &[
    ("id", "Indonesian", "indonesia"),
    ("en", "English", "inggris"),
    ("af", "Afrikaans", ""),
    ("sq", "Albanian", "albania"),
    ("am", "Amharic", "amharik"),
    ("ar", "Arabic", "arab"),
    ("ban", "Balinese", "bali"),
    ("bn", "Bengali", "bengali"),
    ("bg", "Bulgarian", "bulgaria"),
    ("ca", "Catalan", "katalan"),
    ("zh-CN", "Chinese (Simplified)", "mandarin sederhana"),
    ("zh-TW", "Chinese (Traditional)", "mandarin tradisional"),
    ("hr", "Croatian", "kroasia"),
    ("cs", "Czech", "ceko"),
    ("da", "Danish", "denmark"),
    ("nl", "Dutch", "belanda"),
    ("et", "Estonian", "estonia"),
    ("fil", "Filipino", "filipina tagalog"),
    ("fi", "Finnish", "finlandia"),
    ("fr", "French", "prancis perancis"),
    ("de", "German", "jerman"),
    ("el", "Greek", "yunani"),
    ("gu", "Gujarati", ""),
    ("iw", "Hebrew", "ibrani"),
    ("hi", "Hindi", "india"),
    ("hu", "Hungarian", "hungaria"),
    ("is", "Icelandic", "islandia"),
    ("ga", "Irish", "irlandia"),
    ("it", "Italian", "italia"),
    ("ja", "Japanese", "jepang"),
    ("jw", "Javanese", "jawa"),
    ("km", "Khmer", "kamboja"),
    ("ko", "Korean", "korea"),
    ("lo", "Lao", "laos"),
    ("ms", "Malay", "melayu malaysia"),
    ("ml", "Malayalam", ""),
    ("mr", "Marathi", ""),
    ("my", "Myanmar (Burmese)", "burma"),
    ("ne", "Nepali", "nepal"),
    ("no", "Norwegian", "norwegia"),
    ("fa", "Persian", "persia farsi"),
    ("pl", "Polish", "polandia"),
    ("pt", "Portuguese", "portugis"),
    ("pa", "Punjabi", "punjab"),
    ("ro", "Romanian", "rumania"),
    ("ru", "Russian", "rusia"),
    ("sr", "Serbian", "serbia"),
    ("sk", "Slovak", "slovakia"),
    ("es", "Spanish", "spanyol"),
    ("su", "Sundanese", "sunda"),
    ("sw", "Swahili", ""),
    ("sv", "Swedish", "swedia"),
    ("ta", "Tamil", ""),
    ("te", "Telugu", ""),
    ("th", "Thai", "thailand"),
    ("tr", "Turkish", "turki"),
    ("uk", "Ukrainian", "ukraina"),
    ("ur", "Urdu", ""),
    ("vi", "Vietnamese", "vietnam"),
];
