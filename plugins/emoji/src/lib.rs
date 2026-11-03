use std::{
    ffi::{c_char, c_void, CStr},
    panic::{catch_unwind, AssertUnwindSafe},
};
use waymenu_plugin_api::{
    free_json, into_json, PluginAction, PluginApiV2, PluginResult, ABI_VERSION,
};

struct State;

#[unsafe(no_mangle)]
pub extern "C" fn waymenu_plugin_v2() -> PluginApiV2 {
    PluginApiV2 {
        abi_version: ABI_VERSION,
        id: c"emoji".as_ptr(),
        prefix: c"e ".as_ptr(),
        name: c"Emoji Picker".as_ptr(),
        description: c"Search and copy emojis to clipboard.".as_ptr(),
        create: Some(create),
        destroy: Some(destroy),
        search: Some(search),
        action: Some(action),
        free_json: Some(free_json),
    }
}

unsafe extern "C" fn create() -> *mut c_void {
    catch_unwind(|| Box::into_raw(Box::new(State)).cast()).unwrap_or(std::ptr::null_mut())
}

unsafe extern "C" fn destroy(state: *mut c_void) {
    if !state.is_null() {
        drop(unsafe { Box::from_raw(state.cast::<State>()) });
    }
}

unsafe extern "C" fn search(state: *mut c_void, query: *const c_char) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        if state.is_null() || query.is_null() {
            return into_json(&Vec::<PluginResult>::new());
        }
        let Ok(raw_query) = (unsafe { CStr::from_ptr(query) }).to_str() else {
            return into_json(&Vec::<PluginResult>::new());
        };
        into_json(&search_emojis(raw_query))
    }))
    .unwrap_or(std::ptr::null_mut())
}

unsafe extern "C" fn action(_state: *mut c_void, result_id: *const c_char) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        if result_id.is_null() {
            return into_json(&PluginAction::None);
        }
        let Ok(emoji) = (unsafe { CStr::from_ptr(result_id) }).to_str() else {
            return into_json(&PluginAction::None);
        };
        into_json(&PluginAction::CopyText(emoji.to_owned()))
    }))
    .unwrap_or(std::ptr::null_mut())
}

fn ordered_match(candidate: &str, query: &str) -> bool {
    let mut candidate = candidate.chars();
    query
        .chars()
        .all(|needle| candidate.any(|value| value == needle))
}

fn search_emojis(query: &str) -> Vec<PluginResult> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return EMOJIS
            .iter()
            .take(50)
            .map(|(emoji, name, keywords, category)| PluginResult {
                id: (*emoji).to_owned(),
                title: format!("{emoji}  {name}"),
                description: format!("{category} · {keywords}"),
                score: 10_000,
                icon: None,
                image: None,
                metadata: Some("emoji".into()),
            })
            .collect();
    }

    let mut matches = Vec::new();
    for &(emoji, name, keywords, category) in EMOJIS {
        let name_lower = name.to_lowercase();
        let keywords_lower = keywords.to_lowercase();

        let mut score = 0;
        if name_lower == query || emoji == query {
            score = 10_000;
        } else if name_lower.starts_with(&query) {
            score = 9_500;
        } else if name_lower.split_whitespace().any(|word| word == query) {
            score = 9_000;
        } else if keywords_lower.split(',').any(|k| k.trim() == query) {
            score = 8_500;
        } else if name_lower
            .split_whitespace()
            .any(|word| word.starts_with(&query))
        {
            score = 7_500;
        } else if keywords_lower
            .split(',')
            .any(|k| k.trim().starts_with(&query))
        {
            score = 6_500;
        } else if name_lower.contains(&query) {
            score = 5_000;
        } else if keywords_lower.contains(&query) {
            score = 2_000;
        } else if ordered_match(&name_lower, &query) {
            score = 500;
        } else if ordered_match(&keywords_lower, &query) {
            score = 200;
        }

        if score > 0 {
            matches.push((score, emoji, name, keywords, category));
        }
    }

    matches.sort_by_key(|a| std::cmp::Reverse(a.0));
    matches
        .into_iter()
        .take(50)
        .map(|(score, emoji, name, keywords, category)| PluginResult {
            id: emoji.to_owned(),
            title: format!("{emoji}  {name}"),
            description: format!("{category} · {keywords}"),
            score,
            icon: None,
            image: None,
            metadata: Some("emoji".into()),
        })
        .collect()
}

pub static EMOJIS: &[(&str, &str, &str, &str)] = &[
    // Smileys & Emotion
    (
        "😀",
        "Grinning Face",
        "smile, happy, grinning, face",
        "Smileys & Emotion",
    ),
    (
        "😃",
        "Grinning Face with Big Eyes",
        "smile, happy, joyful, eyes",
        "Smileys & Emotion",
    ),
    (
        "😄",
        "Grinning Face with Smiling Eyes",
        "smile, happy, laugh",
        "Smileys & Emotion",
    ),
    (
        "😁",
        "Beaming Face with Smiling Eyes",
        "smile, beaming, grin",
        "Smileys & Emotion",
    ),
    (
        "😆",
        "Grinning Squinting Face",
        "laugh, haha, lol, squinting",
        "Smileys & Emotion",
    ),
    (
        "😅",
        "Grinning Face with Sweat",
        "sweat, nervous, relief, laugh",
        "Smileys & Emotion",
    ),
    (
        "🤣",
        "Rolling on the Floor Laughing",
        "rofl, lol, laughing, floor",
        "Smileys & Emotion",
    ),
    (
        "😂",
        "Face with Tears of Joy",
        "laugh, tears, joy, lol, haha",
        "Smileys & Emotion",
    ),
    (
        "🙂",
        "Slightly Smiling Face",
        "smile, slight, happy",
        "Smileys & Emotion",
    ),
    (
        "🙃",
        "Upside-Down Face",
        "sarcasm, irony, upside down",
        "Smileys & Emotion",
    ),
    (
        "😉",
        "Winking Face",
        "wink, flirt, joke",
        "Smileys & Emotion",
    ),
    (
        "😊",
        "Smiling Face with Smiling Eyes",
        "blush, smile, happy, warm",
        "Smileys & Emotion",
    ),
    (
        "😇",
        "Smiling Face with Halo",
        "angel, innocent, halo",
        "Smileys & Emotion",
    ),
    (
        "🥰",
        "Smiling Face with Hearts",
        "love, adore, hearts, crush",
        "Smileys & Emotion",
    ),
    (
        "😍",
        "Smiling Face with Heart-Eyes",
        "love, heart eyes, adore",
        "Smileys & Emotion",
    ),
    (
        "🤩",
        "Star-Struck",
        "star, wow, amazed, excited",
        "Smileys & Emotion",
    ),
    (
        "😘",
        "Face Blowing a Kiss",
        "kiss, love, blow",
        "Smileys & Emotion",
    ),
    ("😗", "Kissing Face", "kiss, whistle", "Smileys & Emotion"),
    (
        "😚",
        "Kissing Face with Closed Eyes",
        "kiss, blush, closed eyes",
        "Smileys & Emotion",
    ),
    (
        "😙",
        "Kissing Face with Smiling Eyes",
        "kiss, smile, whistle",
        "Smileys & Emotion",
    ),
    (
        "😋",
        "Face Savoring Food",
        "delicious, yummy, tongue, savoring",
        "Smileys & Emotion",
    ),
    (
        "😛",
        "Face with Tongue",
        "tongue, silly, playful",
        "Smileys & Emotion",
    ),
    (
        "😜",
        "Winking Face with Tongue",
        "wink, tongue, crazy, joke",
        "Smileys & Emotion",
    ),
    (
        "🤪",
        "Zany Face",
        "zany, crazy, wild, goofy",
        "Smileys & Emotion",
    ),
    (
        "😝",
        "Squinting Face with Tongue",
        "squint, tongue, playful",
        "Smileys & Emotion",
    ),
    (
        "🤑",
        "Money-Mouth Face",
        "money, rich, dollar, cash",
        "Smileys & Emotion",
    ),
    (
        "🤗",
        "Smiling Face with Open Hands",
        "hug, open hands, embrace",
        "Smileys & Emotion",
    ),
    (
        "🤭",
        "Face with Hand Over Mouth",
        "giggle, oops, secret",
        "Smileys & Emotion",
    ),
    (
        "🤫",
        "Shushing Face",
        "quiet, shh, secret, silence",
        "Smileys & Emotion",
    ),
    (
        "🤔",
        "Thinking Face",
        "think, ponder, hmm, curious",
        "Smileys & Emotion",
    ),
    (
        "🤐",
        "Zipper-Mouth Face",
        "zipper, secret, silent, quiet",
        "Smileys & Emotion",
    ),
    (
        "🤨",
        "Face with Raised Eyebrow",
        "skeptical, doubt, eyebrow",
        "Smileys & Emotion",
    ),
    (
        "😐",
        "Neutral Face",
        "neutral, meh, poker face",
        "Smileys & Emotion",
    ),
    (
        "😑",
        "Expressionless Face",
        "expressionless, unamused, blank",
        "Smileys & Emotion",
    ),
    (
        "😶",
        "Face Without Mouth",
        "silent, mute, speechless",
        "Smileys & Emotion",
    ),
    (
        "😏",
        "Smirking Face",
        "smirk, sneaky, flirt",
        "Smileys & Emotion",
    ),
    (
        "😒",
        "Unamused Face",
        "unamused, bored, annoyed",
        "Smileys & Emotion",
    ),
    (
        "🙄",
        "Face with Rolling Eyes",
        "eye roll, whatever, annoyed",
        "Smileys & Emotion",
    ),
    (
        "😬",
        "Grimacing Face",
        "grimace, awkward, nervous, cringe",
        "Smileys & Emotion",
    ),
    (
        "🤥",
        "Lying Face",
        "pinocchio, lie, fake",
        "Smileys & Emotion",
    ),
    (
        "😌",
        "Relieved Face",
        "relieved, calm, peaceful",
        "Smileys & Emotion",
    ),
    (
        "😔",
        "Pensive Face",
        "sad, pensive, sorrow",
        "Smileys & Emotion",
    ),
    (
        "😪",
        "Sleepy Face",
        "sleep, tired, snooze",
        "Smileys & Emotion",
    ),
    (
        "🤤",
        "Drooling Face",
        "drool, craving, delicious",
        "Smileys & Emotion",
    ),
    (
        "😴",
        "Sleeping Face",
        "sleep, zzz, bed, nap",
        "Smileys & Emotion",
    ),
    (
        "😷",
        "Face with Medical Mask",
        "mask, sick, virus, doctor",
        "Smileys & Emotion",
    ),
    (
        "🤒",
        "Face with Thermometer",
        "sick, fever, ill, thermometer",
        "Smileys & Emotion",
    ),
    (
        "🤕",
        "Face with Head-Bandage",
        "hurt, bandage, injury, pain",
        "Smileys & Emotion",
    ),
    (
        "🤢",
        "Nauseated Face",
        "nauseated, gross, sick, vomit",
        "Smileys & Emotion",
    ),
    (
        "🤮",
        "Face Vomiting",
        "vomit, puke, sick, gross",
        "Smileys & Emotion",
    ),
    (
        "🤧",
        "Sneezing Face",
        "sneeze, tissue, sick, cold",
        "Smileys & Emotion",
    ),
    (
        "🥵",
        "Hot Face",
        "hot, sweating, heat, fever",
        "Smileys & Emotion",
    ),
    (
        "🥶",
        "Cold Face",
        "cold, freezing, frost, ice",
        "Smileys & Emotion",
    ),
    (
        "🥴",
        "Woozy Face",
        "woozy, drunk, dizzy, unsteady",
        "Smileys & Emotion",
    ),
    (
        "😵",
        "Dizzy Face",
        "dizzy, passed out, knockout",
        "Smileys & Emotion",
    ),
    (
        "🤯",
        "Exploding Head",
        "mind blown, shocked, explosion",
        "Smileys & Emotion",
    ),
    (
        "🤠",
        "Cowboy Hat Face",
        "cowboy, western, hat",
        "Smileys & Emotion",
    ),
    (
        "🥳",
        "Partying Face",
        "party, celebrate, birthday, horn",
        "Smileys & Emotion",
    ),
    (
        "😎",
        "Smiling Face with Sunglasses",
        "cool, sunglasses, swag, sunny",
        "Smileys & Emotion",
    ),
    (
        "🤓",
        "Nerd Face",
        "nerd, glasses, smart, geek",
        "Smileys & Emotion",
    ),
    (
        "🧐",
        "Face with Monocle",
        "monocle, detective, examine, fancy",
        "Smileys & Emotion",
    ),
    (
        "😕",
        "Confused Face",
        "confused, puzzled, doubt",
        "Smileys & Emotion",
    ),
    (
        "😟",
        "Worried Face",
        "worried, nervous, anxious",
        "Smileys & Emotion",
    ),
    (
        "🙁",
        "Slightly Frowning Face",
        "frown, sad, unhappy",
        "Smileys & Emotion",
    ),
    (
        "😮",
        "Face with Open Mouth",
        "open mouth, surprise, wow",
        "Smileys & Emotion",
    ),
    (
        "😯",
        "Hushed Face",
        "hushed, surprise, stunned",
        "Smileys & Emotion",
    ),
    (
        "😲",
        "Astonished Face",
        "astonished, shocked, stunned",
        "Smileys & Emotion",
    ),
    (
        "😳",
        "Flushed Face",
        "flushed, embarrassed, blush",
        "Smileys & Emotion",
    ),
    (
        "🥺",
        "Pleading Face",
        "pleading, puppy eyes, begged",
        "Smileys & Emotion",
    ),
    (
        "😦",
        "Frowning Face with Open Mouth",
        "frown, open mouth, shock",
        "Smileys & Emotion",
    ),
    (
        "😧",
        "Anguished Face",
        "anguished, stunned, worried",
        "Smileys & Emotion",
    ),
    (
        "😨",
        "Fearful Face",
        "fear, scared, terrified",
        "Smileys & Emotion",
    ),
    (
        "😰",
        "Anxious Face with Sweat",
        "anxious, sweat, nervous",
        "Smileys & Emotion",
    ),
    (
        "😥",
        "Sad but Relieved Face",
        "sad, relieved, sweat, phew",
        "Smileys & Emotion",
    ),
    ("😢", "Crying Face", "cry, tear, sad", "Smileys & Emotion"),
    (
        "😭",
        "Loudly Crying Face",
        "crying, sob, bawl, tears",
        "Smileys & Emotion",
    ),
    (
        "😱",
        "Face Screaming in Fear",
        "scream, fear, horror, munch",
        "Smileys & Emotion",
    ),
    (
        "😖",
        "Confounded Face",
        "confounded, struggle, upset",
        "Smileys & Emotion",
    ),
    (
        "😣",
        "Persevering Face",
        "persevere, struggle, endure",
        "Smileys & Emotion",
    ),
    (
        "😞",
        "Disappointed Face",
        "disappointed, sad, letdown",
        "Smileys & Emotion",
    ),
    (
        "😓",
        "Downcast Face with Sweat",
        "downcast, sweat, tired",
        "Smileys & Emotion",
    ),
    (
        "😩",
        "Weary Face",
        "weary, tired, whine, moan",
        "Smileys & Emotion",
    ),
    (
        "😫",
        "Tired Face",
        "tired, exhausted, upset",
        "Smileys & Emotion",
    ),
    (
        "🥱",
        "Yawning Face",
        "yawn, tired, sleepy, bored",
        "Smileys & Emotion",
    ),
    (
        "😤",
        "Face with Steam From Nose",
        "triumph, proud, steam, angry",
        "Smileys & Emotion",
    ),
    (
        "😡",
        "Enraged Face",
        "angry, mad, hate, rage, red",
        "Smileys & Emotion",
    ),
    (
        "😠",
        "Angry Face",
        "angry, mad, annoyed",
        "Smileys & Emotion",
    ),
    (
        "🤬",
        "Face with Symbols on Mouth",
        "cursing, swearing, profanity, anger",
        "Smileys & Emotion",
    ),
    (
        "😈",
        "Smiling Face with Horns",
        "devil, evil, naughty, horns",
        "Smileys & Emotion",
    ),
    (
        "👿",
        "Angry Face with Horns",
        "demon, angry, devil, imp",
        "Smileys & Emotion",
    ),
    (
        "💀",
        "Skull",
        "dead, death, skeleton, dying",
        "Smileys & Emotion",
    ),
    (
        "☠",
        "Skull and Crossbones",
        "danger, poison, pirate, death",
        "Smileys & Emotion",
    ),
    ("💩", "Pile of Poo", "poo, poop, crap", "Smileys & Emotion"),
    (
        "🤡",
        "Clown Face",
        "clown, circus, silly, fool",
        "Smileys & Emotion",
    ),
    (
        "👹",
        "Ogre",
        "ogre, monster, japanese, red",
        "Smileys & Emotion",
    ),
    ("👺", "Goblin", "goblin, mask, tengu", "Smileys & Emotion"),
    (
        "👻",
        "Ghost",
        "ghost, spooky, halloween, spirit",
        "Smileys & Emotion",
    ),
    (
        "👽",
        "Alien",
        "alien, ufo, extraterrestrial",
        "Smileys & Emotion",
    ),
    (
        "👾",
        "Alien Monster",
        "space invader, arcade, retro",
        "Smileys & Emotion",
    ),
    (
        "🤖",
        "Robot",
        "robot, bot, tech, android",
        "Smileys & Emotion",
    ),
    (
        "😺",
        "Grinning Cat",
        "cat, grin, happy, kitten",
        "Smileys & Emotion",
    ),
    (
        "😸",
        "Grinning Cat with Smiling Eyes",
        "cat, smile, kitten",
        "Smileys & Emotion",
    ),
    (
        "😹",
        "Cat with Tears of Joy",
        "cat, laugh, lol, tears",
        "Smileys & Emotion",
    ),
    (
        "😻",
        "Smiling Cat with Heart-Eyes",
        "cat, love, heart eyes",
        "Smileys & Emotion",
    ),
    (
        "😼",
        "Cat with Wry Smile",
        "cat, smirk, wry, cheeky",
        "Smileys & Emotion",
    ),
    (
        "😽",
        "Kissing Cat",
        "cat, kiss, kitten",
        "Smileys & Emotion",
    ),
    (
        "🙀",
        "Weary Cat",
        "cat, scream, horror, weary",
        "Smileys & Emotion",
    ),
    (
        "😿",
        "Crying Cat",
        "cat, cry, tear, sad",
        "Smileys & Emotion",
    ),
    (
        "😾",
        "Pouting Cat",
        "cat, pout, angry, grumpy",
        "Smileys & Emotion",
    ),
    (
        "💋",
        "Kiss Mark",
        "kiss, lips, romance",
        "Smileys & Emotion",
    ),
    (
        "💌",
        "Love Letter",
        "letter, envelope, love, romance",
        "Smileys & Emotion",
    ),
    (
        "💘",
        "Heart with Arrow",
        "cupid, love, romance, arrow",
        "Smileys & Emotion",
    ),
    (
        "💝",
        "Heart with Ribbon",
        "gift, ribbon, valentine, love",
        "Smileys & Emotion",
    ),
    (
        "💖",
        "Sparkling Heart",
        "sparkle, glitter, love, heart",
        "Smileys & Emotion",
    ),
    (
        "💗",
        "Growing Heart",
        "growing, pulse, love, heart",
        "Smileys & Emotion",
    ),
    (
        "💓",
        "Beating Heart",
        "heartbeat, pulse, love, heart",
        "Smileys & Emotion",
    ),
    (
        "💞",
        "Revolving Hearts",
        "revolving, swirling, love",
        "Smileys & Emotion",
    ),
    (
        "💕",
        "Two Hearts",
        "two hearts, love, affection",
        "Smileys & Emotion",
    ),
    (
        "💟",
        "Heart Decoration",
        "heart, purple, decoration",
        "Smileys & Emotion",
    ),
    (
        "💔",
        "Broken Heart",
        "broken, heartbreak, breakup, sad",
        "Smileys & Emotion",
    ),
    (
        "❤️",
        "Red Heart",
        "heart, love, romance, red",
        "Smileys & Emotion",
    ),
    (
        "🧡",
        "Orange Heart",
        "heart, love, orange",
        "Smileys & Emotion",
    ),
    (
        "💛",
        "Yellow Heart",
        "heart, love, friendship, yellow",
        "Smileys & Emotion",
    ),
    (
        "💚",
        "Green Heart",
        "heart, love, envy, nature, green",
        "Smileys & Emotion",
    ),
    (
        "💙",
        "Blue Heart",
        "heart, love, peace, blue",
        "Smileys & Emotion",
    ),
    (
        "💜",
        "Purple Heart",
        "heart, love, purple",
        "Smileys & Emotion",
    ),
    (
        "🤎",
        "Brown Heart",
        "heart, love, brown",
        "Smileys & Emotion",
    ),
    (
        "🖤",
        "Black Heart",
        "heart, dark, evil, black",
        "Smileys & Emotion",
    ),
    (
        "🤍",
        "White Heart",
        "heart, pure, peace, white",
        "Smileys & Emotion",
    ),
    (
        "💯",
        "Hundred Points",
        "100, score, perfect, exam",
        "Smileys & Emotion",
    ),
    (
        "💢",
        "Anger Symbol",
        "anger, mad, comic",
        "Smileys & Emotion",
    ),
    (
        "💥",
        "Collision",
        "boom, explosion, collision, impact",
        "Smileys & Emotion",
    ),
    ("💫", "Dizzy", "star, dizzy, sparkle", "Smileys & Emotion"),
    (
        "💦",
        "Sweat Droplets",
        "sweat, water, droplets, work",
        "Smileys & Emotion",
    ),
    (
        "💨",
        "Dashing Away",
        "dash, running, fast, wind, fart",
        "Smileys & Emotion",
    ),
    ("🕳", "Hole", "hole, dark, pit", "Smileys & Emotion"),
    (
        "💣",
        "Bomb",
        "bomb, explode, boom, weapon",
        "Smileys & Emotion",
    ),
    (
        "💬",
        "Speech Balloon",
        "speech, chat, talk, message",
        "Smileys & Emotion",
    ),
    (
        "💭",
        "Thought Balloon",
        "thought, dream, think, bubble",
        "Smileys & Emotion",
    ),
    (
        "💤",
        "Zzz",
        "sleep, zzz, snoring, tired",
        "Smileys & Emotion",
    ),
    // People & Gestures
    (
        "👋",
        "Waving Hand",
        "wave, hello, goodbye, hand",
        "People & Body",
    ),
    (
        "🤚",
        "Raised Back of Hand",
        "hand, raised, back",
        "People & Body",
    ),
    (
        "🖐",
        "Hand with Fingers Splayed",
        "hand, five, splayed",
        "People & Body",
    ),
    (
        "✋",
        "Raised Hand",
        "high five, stop, hand, halt",
        "People & Body",
    ),
    (
        "🖖",
        "Vulcan Salute",
        "spock, star trek, vulcan",
        "People & Body",
    ),
    ("👌", "OK Hand", "ok, perfect, agree, hand", "People & Body"),
    (
        "🤌",
        "Pinched Fingers",
        "italian, gesture, hand, pinched",
        "People & Body",
    ),
    (
        "🤏",
        "Pinching Hand",
        "small, tiny, pinch, little",
        "People & Body",
    ),
    (
        "✌",
        "Victory Hand",
        "peace, victory, two, v sign",
        "People & Body",
    ),
    (
        "🤞",
        "Crossed Fingers",
        "luck, hopeful, promise, fingers crossed",
        "People & Body",
    ),
    (
        "🤟",
        "Love-You Gesture",
        "ily, love you, sign language",
        "People & Body",
    ),
    (
        "🤘",
        "Sign of the Horns",
        "rock on, metal, horns, rock",
        "People & Body",
    ),
    (
        "🤙",
        "Call Me Hand",
        "call, shaka, hang loose",
        "People & Body",
    ),
    (
        "👈",
        "Backhand Index Pointing Left",
        "left, point, index",
        "People & Body",
    ),
    (
        "👉",
        "Backhand Index Pointing Right",
        "right, point, index",
        "People & Body",
    ),
    (
        "👆",
        "Backhand Index Pointing Up",
        "up, point, index",
        "People & Body",
    ),
    ("🖕", "Middle Finger", "fu, finger, rude", "People & Body"),
    (
        "👇",
        "Backhand Index Pointing Down",
        "down, point, index",
        "People & Body",
    ),
    (
        "☝",
        "Index Pointing Up",
        "point, up, one, index",
        "People & Body",
    ),
    (
        "👍",
        "Thumbs Up",
        "like, approve, ok, good, thumbs up",
        "People & Body",
    ),
    (
        "👎",
        "Thumbs Down",
        "dislike, disapprove, bad, no",
        "People & Body",
    ),
    (
        "✊",
        "Raised Fist",
        "fist, power, protest, punch",
        "People & Body",
    ),
    (
        "👊",
        "Oncoming Fist",
        "fist bump, punch, brofist",
        "People & Body",
    ),
    ("🤛", "Left-Facing Fist", "fist bump, left", "People & Body"),
    (
        "🤜",
        "Right-Facing Fist",
        "fist bump, right",
        "People & Body",
    ),
    (
        "👏",
        "Clapping Hands",
        "clap, applause, praise, bravo",
        "People & Body",
    ),
    (
        "🙌",
        "Raising Hands",
        "celebrate, praise, hooray, hands",
        "People & Body",
    ),
    ("👐", "Open Hands", "open, embrace, hands", "People & Body"),
    (
        "🤲",
        "Palms Up Together",
        "pray, du'a, offering, palms",
        "People & Body",
    ),
    (
        "🤝",
        "Handshake",
        "agreement, deal, shake, meeting",
        "People & Body",
    ),
    (
        "🙏",
        "Folded Hands",
        "please, thank you, pray, namaste",
        "People & Body",
    ),
    ("✍", "Writing Hand", "write, memo, pen", "People & Body"),
    (
        "💅",
        "Nail Polish",
        "manicure, nail, beauty, slay",
        "People & Body",
    ),
    ("🤳", "Selfie", "selfie, camera, phone", "People & Body"),
    (
        "💪",
        "Flexed Biceps",
        "muscle, strong, flex, power",
        "People & Body",
    ),
    ("👂", "Ear", "listen, hear, ear", "People & Body"),
    ("👃", "Nose", "smell, sniff, nose", "People & Body"),
    ("🧠", "Brain", "brain, smart, think, mind", "People & Body"),
    ("👀", "Eyes", "look, see, stare, eyes", "People & Body"),
    ("👁", "Eye", "look, see, vision, watch", "People & Body"),
    ("👅", "Tongue", "taste, lick, tongue", "People & Body"),
    ("👄", "Mouth", "lips, kiss, mouth", "People & Body"),
    // Animals & Nature
    (
        "🐶",
        "Dog Face",
        "dog, puppy, pet, canine",
        "Animals & Nature",
    ),
    (
        "🐱",
        "Cat Face",
        "cat, kitten, pet, feline",
        "Animals & Nature",
    ),
    ("🐭", "Mouse Face", "mouse, rodent", "Animals & Nature"),
    ("🐹", "Hamster", "hamster, pet, cute", "Animals & Nature"),
    (
        "🐰",
        "Rabbit Face",
        "rabbit, bunny, easter",
        "Animals & Nature",
    ),
    ("🦊", "Fox", "fox, animal, nature", "Animals & Nature"),
    ("🐻", "Bear", "bear, animal, grizzly", "Animals & Nature"),
    ("🐼", "Panda", "panda, bear, bamboo", "Animals & Nature"),
    ("🐨", "Koala", "koala, australia, cute", "Animals & Nature"),
    ("🐯", "Tiger Face", "tiger, wild, cat", "Animals & Nature"),
    ("🦁", "Lion", "lion, king, roar, cat", "Animals & Nature"),
    ("🐮", "Cow Face", "cow, beef, milk", "Animals & Nature"),
    ("🐷", "Pig Face", "pig, pork, oink", "Animals & Nature"),
    ("🐸", "Frog", "frog, toad, ribbit", "Animals & Nature"),
    (
        "🐵",
        "Monkey Face",
        "monkey, ape, chimp",
        "Animals & Nature",
    ),
    (
        "🙈",
        "See-No-Evil Monkey",
        "monkey, see no evil, hide",
        "Animals & Nature",
    ),
    (
        "🙉",
        "Hear-No-Evil Monkey",
        "monkey, hear no evil",
        "Animals & Nature",
    ),
    (
        "🙊",
        "Speak-No-Evil Monkey",
        "monkey, speak no evil, secret",
        "Animals & Nature",
    ),
    ("🐔", "Chicken", "chicken, hen, bird", "Animals & Nature"),
    (
        "🐧",
        "Penguin",
        "penguin, linux, bird, cold",
        "Animals & Nature",
    ),
    ("🐦", "Bird", "bird, tweet, fly", "Animals & Nature"),
    ("🦅", "Eagle", "eagle, bird, freedom", "Animals & Nature"),
    ("🦆", "Duck", "duck, quack, bird", "Animals & Nature"),
    (
        "🦉",
        "Owl",
        "owl, wise, nocturnal, bird",
        "Animals & Nature",
    ),
    ("🦇", "Bat", "bat, vampire, spooky", "Animals & Nature"),
    ("🐺", "Wolf", "wolf, howl, wild", "Animals & Nature"),
    (
        "🐴",
        "Horse Face",
        "horse, equestrian, pony",
        "Animals & Nature",
    ),
    (
        "🦄",
        "Unicorn",
        "unicorn, fantasy, magic",
        "Animals & Nature",
    ),
    (
        "🐝",
        "Honeybee",
        "bee, honey, insect, buzz",
        "Animals & Nature",
    ),
    ("🐛", "Bug", "bug, caterpillar, insect", "Animals & Nature"),
    (
        "🦋",
        "Butterfly",
        "butterfly, insect, pretty",
        "Animals & Nature",
    ),
    ("🐌", "Snail", "snail, slow, shell", "Animals & Nature"),
    (
        "🐞",
        "Lady Beetle",
        "ladybug, beetle, lucky",
        "Animals & Nature",
    ),
    ("🐜", "Ant", "ant, insect, tiny", "Animals & Nature"),
    ("🕷", "Spider", "spider, arachnid, web", "Animals & Nature"),
    (
        "🦂",
        "Scorpion",
        "scorpion, sting, desert",
        "Animals & Nature",
    ),
    ("🐢", "Turtle", "turtle, slow, tortoise", "Animals & Nature"),
    ("🐍", "Snake", "snake, serpent, python", "Animals & Nature"),
    ("🦎", "Lizard", "lizard, reptile, gecko", "Animals & Nature"),
    (
        "🐙",
        "Octopus",
        "octopus, tentacle, ocean",
        "Animals & Nature",
    ),
    ("🦑", "Squid", "squid, ocean, tentacle", "Animals & Nature"),
    ("🦐", "Shrimp", "shrimp, seafood, prawn", "Animals & Nature"),
    (
        "🦞",
        "Lobster",
        "lobster, seafood, crab",
        "Animals & Nature",
    ),
    (
        "🦀",
        "Crab",
        "crab, seafood, crustacean, rust",
        "Animals & Nature",
    ),
    (
        "🐡",
        "Blowfish",
        "blowfish, pufferfish, ocean",
        "Animals & Nature",
    ),
    (
        "🐠",
        "Tropical Fish",
        "fish, tropical, swim",
        "Animals & Nature",
    ),
    ("🐟", "Fish", "fish, swim, sea", "Animals & Nature"),
    (
        "🐬",
        "Dolphin",
        "dolphin, ocean, sea, mammal",
        "Animals & Nature",
    ),
    (
        "🐳",
        "Spouting Whale",
        "whale, spouting, ocean, docker",
        "Animals & Nature",
    ),
    ("🐋", "Whale", "whale, sea, mammal", "Animals & Nature"),
    (
        "🦈",
        "Shark",
        "shark, jaws, predator, sea",
        "Animals & Nature",
    ),
    (
        "🐊",
        "Crocodile",
        "crocodile, alligator, reptile",
        "Animals & Nature",
    ),
    (
        "🐘",
        "Elephant",
        "elephant, trunk, animal",
        "Animals & Nature",
    ),
    (
        "🌲",
        "Evergreen Tree",
        "tree, pine, evergreen, christmas",
        "Animals & Nature",
    ),
    (
        "🌳",
        "Deciduous Tree",
        "tree, park, nature",
        "Animals & Nature",
    ),
    (
        "🌴",
        "Palm Tree",
        "palm, beach, tropical",
        "Animals & Nature",
    ),
    ("🌵", "Cactus", "cactus, desert, plant", "Animals & Nature"),
    (
        "🍀",
        "Four Leaf Clover",
        "clover, lucky, four leaf",
        "Animals & Nature",
    ),
    (
        "🍁",
        "Maple Leaf",
        "maple, leaf, autumn, canada",
        "Animals & Nature",
    ),
    (
        "🍂",
        "Fallen Leaf",
        "leaf, autumn, fall",
        "Animals & Nature",
    ),
    (
        "🍄",
        "Mushroom",
        "mushroom, fungus, toadstool, mario",
        "Animals & Nature",
    ),
    (
        "🌸",
        "Cherry Blossom",
        "cherry blossom, sakura, flower, spring",
        "Animals & Nature",
    ),
    (
        "🌹",
        "Rose",
        "rose, flower, romance, red",
        "Animals & Nature",
    ),
    (
        "🌻",
        "Sunflower",
        "sunflower, yellow, flower",
        "Animals & Nature",
    ),
    ("🔥", "Fire", "fire, flame, hot, lit", "Animals & Nature"),
    ("⭐", "Star", "star, yellow, favorite", "Animals & Nature"),
    (
        "🌟",
        "Glowing Star",
        "glowing, shine, sparkle, star",
        "Animals & Nature",
    ),
    (
        "✨",
        "Sparkles",
        "sparkles, shine, stars, clean, magic",
        "Animals & Nature",
    ),
    (
        "⚡",
        "High Voltage",
        "lightning, bolt, electric, shock, fast",
        "Animals & Nature",
    ),
    (
        "🌈",
        "Rainbow",
        "rainbow, colors, pride",
        "Animals & Nature",
    ),
    (
        "☀️",
        "Sun",
        "sun, sunny, bright, weather",
        "Animals & Nature",
    ),
    ("☁", "Cloud", "cloud, weather, overcast", "Animals & Nature"),
    (
        "🌧",
        "Cloud with Rain",
        "rain, shower, weather",
        "Animals & Nature",
    ),
    (
        "❄",
        "Snowflake",
        "snow, cold, winter, ice",
        "Animals & Nature",
    ),
    (
        "🌙",
        "Crescent Moon",
        "moon, crescent, night",
        "Animals & Nature",
    ),
    (
        "🌍",
        "Globe Showing Europe-Africa",
        "earth, globe, world",
        "Animals & Nature",
    ),
    // Food & Drink
    (
        "☕",
        "Hot Beverage",
        "coffee, tea, drink, cafe",
        "Food & Drink",
    ),
    (
        "🍵",
        "Teacup Without Handle",
        "green tea, matcha, tea",
        "Food & Drink",
    ),
    (
        "🍺",
        "Beer Mug",
        "beer, drink, alcohol, pub",
        "Food & Drink",
    ),
    (
        "🍻",
        "Clinking Beer Mugs",
        "cheers, beer, toast, party",
        "Food & Drink",
    ),
    ("🍷", "Wine Glass", "wine, alcohol, red", "Food & Drink"),
    (
        "🍸",
        "Cocktail Glass",
        "cocktail, martini, drink",
        "Food & Drink",
    ),
    (
        "🍾",
        "Bottle with Popping Cork",
        "champagne, celebrate, pop",
        "Food & Drink",
    ),
    (
        "🍕",
        "Pizza",
        "pizza, cheese, slice, fast food",
        "Food & Drink",
    ),
    ("🍔", "Hamburger", "burger, fast food, beef", "Food & Drink"),
    (
        "🍟",
        "French Fries",
        "fries, chips, fast food",
        "Food & Drink",
    ),
    (
        "🌭",
        "Hot Dog",
        "hotdog, sausage, fast food",
        "Food & Drink",
    ),
    ("🥪", "Sandwich", "sandwich, bread, lunch", "Food & Drink"),
    ("🌮", "Taco", "taco, mexican, food", "Food & Drink"),
    ("🌯", "Burrito", "burrito, mexican, wrap", "Food & Drink"),
    (
        "🍜",
        "Steaming Bowl",
        "ramen, noodles, soup",
        "Food & Drink",
    ),
    ("🍝", "Spaghetti", "pasta, italian, noodles", "Food & Drink"),
    ("🍣", "Sushi", "sushi, japanese, fish, rice", "Food & Drink"),
    (
        "🍙",
        "Rice Ball",
        "onigiri, rice ball, japanese",
        "Food & Drink",
    ),
    ("🍚", "Cooked Rice", "rice, bowl, food", "Food & Drink"),
    (
        "🍰",
        "Shortcake",
        "cake, strawberry, dessert, sweet",
        "Food & Drink",
    ),
    (
        "🎂",
        "Birthday Cake",
        "birthday, cake, candle, celebration",
        "Food & Drink",
    ),
    (
        "🍩",
        "Doughnut",
        "donut, doughnut, sweet, pastry",
        "Food & Drink",
    ),
    (
        "🍪",
        "Cookie",
        "cookie, biscuit, chocolate chip",
        "Food & Drink",
    ),
    (
        "🍫",
        "Chocolate Bar",
        "chocolate, sweet, candy",
        "Food & Drink",
    ),
    (
        "🍿",
        "Popcorn",
        "popcorn, movie, cinema, snack",
        "Food & Drink",
    ),
    ("🍎", "Red Apple", "apple, fruit, red", "Food & Drink"),
    ("🍌", "Banana", "banana, fruit, yellow", "Food & Drink"),
    (
        "🍉",
        "Watermelon",
        "watermelon, fruit, melon",
        "Food & Drink",
    ),
    (
        "🍓",
        "Strawberry",
        "strawberry, berry, fruit",
        "Food & Drink",
    ),
    ("🥑", "Avocado", "avocado, guacamole, fruit", "Food & Drink"),
    // Activities & Sports
    (
        "⚽",
        "Soccer Ball",
        "soccer, football, ball, sport",
        "Activities",
    ),
    (
        "🏀",
        "Basketball",
        "basketball, ball, sport, nba",
        "Activities",
    ),
    (
        "🏈",
        "American Football",
        "football, nfl, sport",
        "Activities",
    ),
    ("⚾", "Baseball", "baseball, ball, sport", "Activities"),
    ("🎾", "Tennis", "tennis, ball, sport, racket", "Activities"),
    (
        "🎮",
        "Video Game",
        "game, controller, console, gaming",
        "Activities",
    ),
    (
        "🎯",
        "Bullseye",
        "target, dart, bullseye, direct",
        "Activities",
    ),
    (
        "🎲",
        "Game Die",
        "dice, die, board game, gamble",
        "Activities",
    ),
    (
        "🏆",
        "Trophy",
        "trophy, prize, winner, champion",
        "Activities",
    ),
    (
        "🥇",
        "1st Place Medal",
        "gold medal, first, winner",
        "Activities",
    ),
    (
        "🥈",
        "2nd Place Medal",
        "silver medal, second",
        "Activities",
    ),
    ("🥉", "3rd Place Medal", "bronze medal, third", "Activities"),
    (
        "🎨",
        "Artist Palette",
        "art, paint, palette, draw",
        "Activities",
    ),
    (
        "🎬",
        "Clapper Board",
        "movie, film, cinema, action",
        "Activities",
    ),
    (
        "🎤",
        "Microphone",
        "mic, sing, karaoke, music",
        "Activities",
    ),
    (
        "🎧",
        "Headphone",
        "headphones, music, listen, audio",
        "Activities",
    ),
    (
        "🎸",
        "Guitar",
        "guitar, rock, music, instrument",
        "Activities",
    ),
    (
        "🎹",
        "Musical Keyboard",
        "piano, keyboard, music",
        "Activities",
    ),
    // Travel & Places
    (
        "🚀",
        "Rocket",
        "rocket, space, launch, fast, blast off",
        "Travel & Places",
    ),
    (
        "✈️",
        "Airplane",
        "plane, flight, travel, fly",
        "Travel & Places",
    ),
    ("🚗", "Automobile", "car, drive, vehicle", "Travel & Places"),
    ("🏎", "Racing Car", "f1, race, car, fast", "Travel & Places"),
    ("🚲", "Bicycle", "bike, cycle, ride", "Travel & Places"),
    (
        "🛵",
        "Motor Scooter",
        "scooter, vespa, moped",
        "Travel & Places",
    ),
    (
        "🏍",
        "Motorcycle",
        "motorcycle, motorbike, ride",
        "Travel & Places",
    ),
    (
        "🚄",
        "High-Speed Train",
        "bullet train, shinkansen, rail",
        "Travel & Places",
    ),
    ("🚢", "Ship", "ship, boat, ocean, cruise", "Travel & Places"),
    ("🏠", "House", "home, house, building", "Travel & Places"),
    (
        "🏢",
        "Office Building",
        "office, work, building",
        "Travel & Places",
    ),
    // Objects
    (
        "💻",
        "Laptop",
        "laptop, computer, pc, work, tech",
        "Objects",
    ),
    (
        "🖥",
        "Desktop Computer",
        "desktop, monitor, computer",
        "Objects",
    ),
    (
        "📱",
        "Mobile Phone",
        "phone, smartphone, mobile, iphone",
        "Objects",
    ),
    ("⌨", "Keyboard", "keyboard, type, computer", "Objects"),
    ("🖱", "Computer Mouse", "mouse, click, computer", "Objects"),
    ("💾", "Floppy Disk", "save, floppy, disk, retro", "Objects"),
    ("📷", "Camera", "camera, photo, picture", "Objects"),
    ("📸", "Camera with Flash", "camera, flash, photo", "Objects"),
    (
        "🔍",
        "Magnifying Glass Tilted Left",
        "search, find, zoom, magnifying glass",
        "Objects",
    ),
    (
        "🔎",
        "Magnifying Glass Tilted Right",
        "search, find, inspect",
        "Objects",
    ),
    ("💡", "Light Bulb", "idea, light, bulb, bright", "Objects"),
    ("🔦", "Flashlight", "flashlight, torch, light", "Objects"),
    ("📖", "Open Book", "book, read, study, learn", "Objects"),
    (
        "📚",
        "Books",
        "books, library, study, literature",
        "Objects",
    ),
    ("📝", "Memo", "memo, note, write, paper", "Objects"),
    ("📁", "File Folder", "folder, directory, file", "Objects"),
    ("📅", "Calendar", "calendar, date, schedule", "Objects"),
    ("📌", "Pushpin", "pin, pushpin, location, notice", "Objects"),
    (
        "📍",
        "Round Pushpin",
        "pin, location, map, marker",
        "Objects",
    ),
    ("📎", "Paperclip", "clip, paperclip, attachment", "Objects"),
    ("✂️", "Scissors", "scissors, cut, shear", "Objects"),
    ("🔒", "Locked", "lock, secure, safe, privacy", "Objects"),
    ("🔓", "Unlocked", "unlock, open, free", "Objects"),
    ("🔑", "Key", "key, unlock, password, access", "Objects"),
    ("🔨", "Hammer", "tool, hammer, build, fix", "Objects"),
    ("🔧", "Wrench", "tool, wrench, fix, settings", "Objects"),
    ("⚙️", "Gear", "gear, settings, cog, options", "Objects"),
    ("🔗", "Link", "link, url, hyperlink, chain", "Objects"),
    ("🔔", "Bell", "bell, notification, alert, ring", "Objects"),
    (
        "🔕",
        "Bell with Slash",
        "silent, mute, notification, off",
        "Objects",
    ),
    (
        "🎁",
        "Wrapped Gift",
        "gift, present, birthday, box",
        "Objects",
    ),
    (
        "🎉",
        "Party Popper",
        "party, celebration, congrats, tada",
        "Objects",
    ),
    (
        "🎊",
        "Confetti Ball",
        "confetti, party, celebrate",
        "Objects",
    ),
    ("🎈", "Balloon", "balloon, party, birthday", "Objects"),
    ("📦", "Package", "box, package, parcel, delivery", "Objects"),
    (
        "✉️",
        "Envelope",
        "mail, email, envelope, message",
        "Objects",
    ),
    (
        "💰",
        "Money Bag",
        "money, bag, rich, cash, dollar",
        "Objects",
    ),
    (
        "💵",
        "Dollar Banknote",
        "dollar, money, cash, bill",
        "Objects",
    ),
    (
        "💳",
        "Credit Card",
        "credit card, pay, payment, buy",
        "Objects",
    ),
    (
        "💎",
        "Gem Stone",
        "diamond, gem, jewel, precious",
        "Objects",
    ),
    ("🗑", "Wastebasket", "trash, delete, garbage, bin", "Objects"),
    // Symbols
    (
        "✅",
        "Check Mark Button",
        "check, tick, done, correct, yes",
        "Symbols",
    ),
    ("✔️", "Check Mark", "check, tick, done, approve", "Symbols"),
    ("❌", "Cross Mark", "cross, x, no, cancel, wrong", "Symbols"),
    (
        "❎",
        "Cross Mark Button",
        "cross, x, square, button",
        "Symbols",
    ),
    (
        "⚠️",
        "Warning",
        "warning, alert, caution, danger",
        "Symbols",
    ),
    ("⛔", "No Entry", "no entry, stop, forbidden", "Symbols"),
    (
        "🚫",
        "Prohibited",
        "prohibited, banned, no, cancel",
        "Symbols",
    ),
    ("ℹ️", "Information", "info, information, help", "Symbols"),
    (
        "❓",
        "Red Question Mark",
        "question, help, ask, why",
        "Symbols",
    ),
    (
        "❗",
        "Red Exclamation Mark",
        "exclamation, mark, important, alert",
        "Symbols",
    ),
    ("▶️", "Play Button", "play, start, video", "Symbols"),
    ("⏸️", "Pause Button", "pause, stop, wait", "Symbols"),
    ("⏹️", "Stop Button", "stop, square", "Symbols"),
    (
        "🔄",
        "Counterclockwise Arrows Button",
        "reload, refresh, sync, update",
        "Symbols",
    ),
    ("➕", "Plus", "plus, add, math", "Symbols"),
    ("➖", "Minus", "minus, subtract, math", "Symbols"),
    ("✖️", "Multiply", "multiply, math, times", "Symbols"),
    ("➗", "Divide", "divide, math", "Symbols"),
    ("♾️", "Infinity", "infinity, loop, forever", "Symbols"),
    (
        "❤️‍🔥",
        "Heart on Fire",
        "passion, desire, fire, love",
        "Symbols",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn exports_expected_prefix_and_metadata() {
        let api = waymenu_plugin_v2();
        let prefix = unsafe { CStr::from_ptr(api.prefix) }.to_str().unwrap();
        assert_eq!(prefix, "e ");
        let id = unsafe { CStr::from_ptr(api.id) }.to_str().unwrap();
        assert_eq!(id, "emoji");
    }

    #[test]
    fn search_finds_emojis_by_keyword_and_name() {
        let results = search_emojis("fire");
        assert!(!results.is_empty());
        assert_eq!(results[0].id, "🔥");

        let results = search_emojis("smile");
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.id == "😀"));

        let results = search_emojis("heart");
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.id == "❤️"));
    }

    #[test]
    fn action_returns_copy_text_with_selected_emoji() {
        let state = unsafe { create() };
        let emoji_id = CString::new("🔥").unwrap();
        let ptr = unsafe { action(state, emoji_id.as_ptr()) };
        assert!(!ptr.is_null());

        let json = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap();
        let action_res: PluginAction = serde_json::from_str(json).unwrap();
        match action_res {
            PluginAction::CopyText(copied) => assert_eq!(copied, "🔥"),
            other => panic!("unexpected action: {other:?}"),
        }

        unsafe { free_json(ptr) };
        unsafe { destroy(state) };
    }
}
