//! Build codenames — a deterministic, friendly handle for every build.
//!
//! Each build gets an `adjective-animal` name derived from its git short sha,
//! so `759eddc` is *always* `amber-otter` — reproducible, not random. Same
//! trick as Docker/Heroku/Ubuntu, but seeded by the sha instead of an RNG, so
//! the name uniquely and stably identifies the commit. 200 × 200 = 40,000
//! combinations → collisions across real builds are effectively nil.
//!
//! Tagging the box and the Mac app from the same commit gives them the same
//! codename — "the amber-otter build" is the matched box+DMG pair.

/// 200 fun adjectives. Single lowercase words so `adj-animal` hyphenates clean.
pub const ADJECTIVES: &[&str] = &[
    "amber", "ample", "brave", "brisk", "bubbly", "chipper", "chirpy", "clever", "cosmic", "cozy",
    "crafty", "crisp", "dapper", "daring", "dazzling", "deft", "dewy", "dreamy", "eager", "earnest",
    "elated", "fabled", "fancy", "feisty", "fizzy", "fleet", "fluffy", "foamy", "frisky", "fuzzy",
    "gallant", "gentle", "giddy", "gleeful", "glossy", "golden", "goofy", "graceful", "grand", "groovy",
    "gusty", "hardy", "hazy", "hearty", "honest", "humble", "jaunty", "jazzy", "jolly", "jovial",
    "joyful", "keen", "kindly", "lanky", "lavish", "lively", "lofty", "lucky", "lush", "mellow",
    "merry", "mighty", "minty", "misty", "nimble", "noble", "peppy", "perky", "placid", "plucky",
    "plump", "polished", "posh", "prim", "proud", "quaint", "quick", "quirky", "radiant", "rapid",
    "rosy", "rugged", "rustic", "sandy", "sassy", "savvy", "scrappy", "serene", "shaggy", "sharp",
    "shiny", "silky", "silly", "sleek", "slick", "snappy", "snug", "soft", "spicy", "spiffy",
    "spirited", "splendid", "spry", "stalwart", "steadfast", "stellar", "sturdy", "sunny", "swift", "tame",
    "tangy", "tidy", "timely", "tiny", "tranquil", "trusty", "twinkly", "upbeat", "valiant", "velvet",
    "vivid", "vibrant", "witty", "woolly", "zany", "zappy", "zesty", "zippy", "breezy", "cheery",
    "dandy", "frosty", "glad", "jumpy", "lemony", "peachy", "pearly", "plush", "sprightly", "wholesome",
    "bouncy", "chunky", "comfy", "cuddly", "dappled", "downy", "fervent", "gilded", "hushed", "intrepid",
    "jubilant", "lustrous", "mirthful", "nifty", "opal", "plummy", "regal", "rascally", "bonny", "dashing",
    "ember", "frolicsome", "gingery", "honeyed", "impish", "jangly", "lilac", "marbled", "nutty", "ochre",
    "peppery", "quizzical", "scarlet", "tawny", "umber", "verdant", "whimsical", "amiable", "blithe", "candid",
    "dauntless", "ebullient", "genial", "hale", "jocund", "limber", "lithe", "mossy", "nippy", "ruddy",
    "speckled", "toasty", "wily", "yappy", "winsome", "spunky", "chummy", "rollicking", "snazzy", "wiggly",
];

/// 200 animals. Single lowercase words.
pub const ANIMALS: &[&str] = &[
    "otter", "heron", "lynx", "ferret", "badger", "marten", "vole", "stoat", "weasel", "mongoose",
    "civet", "genet", "fossa", "quokka", "wombat", "numbat", "dingo", "koala", "possum", "wallaby",
    "fox", "wolf", "bear", "owl", "hawk", "falcon", "raven", "crow", "sparrow", "finch",
    "robin", "wren", "swift", "swallow", "martin", "magpie", "jay", "lark", "thrush", "starling",
    "kestrel", "osprey", "harrier", "kite", "buzzard", "eagle", "vulture", "condor", "puffin", "gannet",
    "tern", "gull", "plover", "sandpiper", "curlew", "snipe", "godwit", "dunlin", "avocet", "lapwing",
    "pelican", "cormorant", "ibis", "egret", "stork", "crane", "flamingo", "spoonbill", "bittern", "rail",
    "moorhen", "coot", "grebe", "loon", "mallard", "teal", "wigeon", "pintail", "shoveler", "merganser",
    "swan", "goose", "gadwall", "eider", "scoter", "smew", "garganey", "pochard", "shelduck", "brant",
    "deer", "elk", "moose", "caribou", "bison", "antelope", "gazelle", "ibex", "chamois", "oryx",
    "hare", "rabbit", "marmot", "gopher", "beaver", "muskrat", "chipmunk", "squirrel", "dormouse", "lemming",
    "hedgehog", "shrew", "mole", "bat", "pangolin", "armadillo", "sloth", "anteater", "aardvark", "tapir",
    "boar", "hog", "peccary", "capybara", "agouti", "paca", "coati", "kinkajou", "raccoon", "skunk",
    "bobcat", "cougar", "ocelot", "serval", "caracal", "jaguar", "leopard", "panther", "cheetah", "lion",
    "tiger", "jackal", "coyote", "dhole", "fennec", "meerkat", "aardwolf", "hyena", "binturong", "dassie",
    "walrus", "seal", "sealion", "dugong", "manatee", "narwhal", "beluga", "orca", "dolphin", "porpoise",
    "marlin", "tuna", "salmon", "trout", "perch", "pike", "carp", "bass", "bream", "tench",
    "gudgeon", "minnow", "roach", "rudd", "dace", "chub", "barbel", "loach", "ide", "burbot",
    "newt", "toad", "frog", "salamander", "axolotl", "gecko", "skink", "iguana", "agama", "chameleon",
    "tortoise", "terrapin", "turtle", "python", "viper", "adder", "cobra", "mamba", "boa", "krait",
];

/// The full `--version` line: release tag + codename + build date + short sha, e.g.
/// `v0.1.0-staging.43 "swift-moorhen" · 2026-06-17 · 759eddc`. Prefers the full git
/// tag (`GIT_DESCRIBE`, so a box shows exactly which staging build it's on) and
/// falls back to the crate semver for tag-less local builds. `GIT_COMMIT`,
/// `GIT_DESCRIBE`, and `BUILD_TIME` are baked by build.rs; the codename is derived
/// from the sha so it's stable per commit. Returned owned so clap can take it.
pub fn long_version() -> String {
    let sha = env!("GIT_COMMIT");
    let short = &sha[..sha.len().min(7)];
    let date = env!("BUILD_TIME").split('T').next().unwrap_or(env!("BUILD_TIME"));
    let describe = env!("GIT_DESCRIBE");
    let version = if describe.is_empty() {
        env!("CARGO_PKG_VERSION")
    } else {
        describe
    };
    format!("{} \"{}\" · {} · {}", version, codename(sha), date, short)
}

/// Deterministic `adjective-animal` from a git short sha (or any hex string).
/// Falls back gracefully on non-hex input.
pub fn codename(sha: &str) -> String {
    let clean: String = sha.chars().filter(|c| c.is_ascii_hexdigit()).take(12).collect();
    let n = u64::from_str_radix(&clean, 16).unwrap_or(0);
    let adj = ADJECTIVES[(n % ADJECTIVES.len() as u64) as usize];
    let animal = ANIMALS[((n / ADJECTIVES.len() as u64) % ANIMALS.len() as u64) as usize];
    format!("{adj}-{animal}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn lists_are_200_and_unique() {
        assert_eq!(ADJECTIVES.len(), 200, "want 200 adjectives");
        assert_eq!(ANIMALS.len(), 200, "want 200 animals");
        let adj: HashSet<_> = ADJECTIVES.iter().collect();
        let ani: HashSet<_> = ANIMALS.iter().collect();
        assert_eq!(adj.len(), 200, "duplicate adjective(s)");
        assert_eq!(ani.len(), 200, "duplicate animal(s)");
    }

    #[test]
    fn deterministic_and_reproducible() {
        assert_eq!(codename("759eddc"), codename("759eddc"));
        assert_eq!(codename("759eddc0a1b2"), codename("759eddc0a1b2"));
        // format is adjective-animal
        let c = codename("deadbeef");
        assert!(c.contains('-'));
        assert!(ADJECTIVES.contains(&c.split('-').next().unwrap()));
    }

    #[test]
    fn non_hex_does_not_panic() {
        let _ = codename("");
        let _ = codename("not-a-sha");
    }
}
