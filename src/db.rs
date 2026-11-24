use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::storage::{GameRecord, Loader};

#[derive(Clone, Debug)]
pub struct DbMatch {

    pub name: String,

    pub game_id: Option<String>,

        pub platform: Option<String>,

    pub image: Option<String>,
    pub loader: Loader,

    pub version: String,

    pub sb_crc: Option<u32>,

    /// The product's place in the database's list of games.
    pub place: usize,
}

struct Db {
    games: Vec<Product>,
    card_reader: Option<Product>,
}

struct Product {
    name: String,
    game_id: Option<String>,
    platform: Option<String>,

    image: Option<String>,
    loader: Loader,
    builds: Vec<Build>,
    /// Whether the game can be played in a room (in-store battles).
    in_store_battles: bool,
    /// The date the game came out in Japan (`YYYY-MM-DD`), which its export versions share.
    release: Option<String>,
}

struct Build {

    crc: Option<u32>,
    version: String,
    sb_crc: Option<u32>,
}

fn db() -> &'static Db {
    static DB: OnceLock<Db> = OnceLock::new();
    DB.get_or_init(|| {
        let raw: RawDb =
            serde_json::from_str(include_str!("../games_db.json")).expect("games_db.json failed to parse");
        raw.into_db()
    })
}

#[derive(Deserialize)]
struct RawDb {
    #[serde(default)]
    games: Vec<RawProduct>,
    #[serde(default)]
    card_reader: Option<RawProduct>,
}

#[derive(Deserialize)]
struct RawProduct {
    #[serde(default)]
    name: String,
    #[serde(default)]
    game_id: Option<String>,
    #[serde(default)]
    platform: Option<String>,

    #[serde(default)]
    image: Option<String>,
    #[serde(default)]
    loader: Option<String>,
    #[serde(default)]
    support_game: Vec<RawBuild>,
    #[serde(default = "playable_in_rooms")]
    in_store_battles: bool,
    #[serde(default)]
    release: Option<String>,
}

/// Products can be played in rooms unless the database says otherwise.
fn playable_in_rooms() -> bool {
    true
}

#[derive(Deserialize)]
struct RawBuild {

    #[serde(default)]
    crc: String,
    #[serde(default)]
    version: String,

    #[serde(default)]
    sb_crc: Option<String>,
}

impl RawDb {
    fn into_db(self) -> Db {

        let parse = |p: &RawProduct, fallback_name: &str| -> Product {
            let name = if p.name.is_empty() {
                fallback_name.to_string()
            } else {
                p.name.clone()
            };
            let loader_str = p.loader.as_deref().unwrap_or("");
            let loader = match parse_loader(loader_str) {
                Some(l) => l,
                None => panic!("games_db.json: product \"{name}\" has an invalid loader \"{loader_str}\""),
            };
            let builds = p
                .support_game
                .iter()
                .map(|b| Build {
                    crc: parse_crc(&b.crc),
                    version: b.version.clone(),
                    sb_crc: b
                        .sb_crc
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .and_then(parse_crc),
                })
                .collect();
            Product {
                name,
                game_id: p.game_id.clone().filter(|v| !v.is_empty()),
                platform: p.platform.clone().filter(|v| !v.is_empty()),
                image: p.image.clone().filter(|v| !v.is_empty()),
                loader,
                builds,
                in_store_battles: p.in_store_battles,
                release: p.release.clone().filter(|v| !v.is_empty()),
            }
        };

        let games = self.games.iter().map(|p| parse(p, "Unnamed game")).collect();
        let card_reader = self.card_reader.as_ref().map(|p| parse(p, "card_reader"));
        Db { games, card_reader }
    }
}

fn parse_loader(s: &str) -> Option<Loader> {
    match s.trim().to_ascii_lowercase().as_str() {
        "kitten" => Some(Loader::Kitten),
        "mice" => Some(Loader::Mice),
        "segatools" => Some(Loader::Segatools),
        _ => None,
    }
}

fn parse_crc(s: &str) -> Option<u32> {
    let t = s.trim();
    let hex = t
        .strip_prefix("0x")
        .or_else(|| t.strip_prefix("0X"))
        .unwrap_or(t);
    u32::from_str_radix(hex, 16).ok()
}

fn crc_index() -> &'static HashMap<(u32, Loader), DbMatch> {
    static IDX: OnceLock<HashMap<(u32, Loader), DbMatch>> = OnceLock::new();
    IDX.get_or_init(|| {
        let mut index: HashMap<(u32, Loader), DbMatch> = HashMap::new();
        for (place, g) in db().games.iter().enumerate() {
            for b in g.builds.iter() {
                let Some(crc) = b.crc else {
                    continue;
                };

                index.entry((crc, g.loader)).or_insert_with(|| DbMatch {
                    name: g.name.clone(),
                    game_id: g.game_id.clone(),
                    platform: g.platform.clone(),
                    image: g.image.clone(),
                    loader: g.loader,
                    version: b.version.clone(),
                    sb_crc: b.sb_crc,
                    place,
                });
            }
        }
        index
    })
}

pub fn lookup_ref(crc: u32, loader: Loader) -> Option<&'static DbMatch> {
    crc_index().get(&(crc, loader))
}

pub fn lookup(crc: u32, loader: Loader) -> Option<DbMatch> {
    lookup_ref(crc, loader).cloned()
}

/// The database entry of a library record, by the CRC it was added with.
pub fn lookup_record(game: &GameRecord) -> Option<&'static DbMatch> {
    let s = game.crc.as_deref()?;
    let hex = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    let crc = u32::from_str_radix(hex.trim(), 16).ok()?;
    lookup_ref(crc, game.loader)
}

pub fn lookup_by_game_id(game_id: &str, version: &str) -> Option<DbMatch> {
    let want = parse_version(version)?;
    db().games.iter().enumerate().find_map(|(place, g)| {
        if g.game_id.as_deref() != Some(game_id) {
            return None;
        }
        g.builds
            .iter()
            .find(|b| parse_version(&b.version) == Some(want))
            .map(|b| DbMatch {
                name: g.name.clone(),
                game_id: g.game_id.clone(),
                platform: g.platform.clone(),
                image: g.image.clone(),
                loader: g.loader,
                version: b.version.clone(),
                sb_crc: b.sb_crc,
                place,
            })
    })
}

/// Where a library game stands among the others: its product's release date, then the
/// product's place in the database, which lists the versions of one game in the order JPN,
/// EXP, CHN. `None` for a game the database does not know.
///
/// The product is the one matched by the CRC the game was added with or, for a game added
/// from its ICF (which has none), by its game id and version.
pub fn release_order(game: &GameRecord) -> Option<(&'static str, usize)> {
    let place = match lookup_record(game) {
        Some(matched) => matched.place,
        None => lookup_by_game_id(game.game_id.as_deref()?, game.version.as_deref()?)?.place,
    };
    Some((db().games[place].release.as_deref()?, place))
}

fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let parts: Vec<&str> = s.trim().split('.').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let mut v = [0u32; 3];
    for (i, p) in parts.iter().enumerate() {
        v[i] = p.trim().parse().ok()?;
    }
    Some((v[0], v[1], v[2]))
}

/// The names of the products that can be played in rooms, in database order.
pub fn room_game_names() -> Vec<String> {
    room_games().into_iter().map(|g| g.name).collect()
}

#[derive(Clone, Debug)]
pub struct GameEntry {
    pub name: String,
    pub game_id: Option<String>,
}

/// The products that can be played in rooms, in database order.
pub fn room_games() -> Vec<GameEntry> {
    db().games
        .iter()
        .filter(|g| g.in_store_battles)
        .map(|g| GameEntry { name: g.name.clone(), game_id: g.game_id.clone() })
        .collect()
}

pub fn match_card_reader(crc: u32) -> Option<String> {
    db().card_reader
        .as_ref()
        .and_then(|p| p.builds.iter().find(|b| b.crc == Some(crc)))
        .map(|b| b.version.clone())
}

#[cfg(test)]
pub fn build_count() -> usize {
    db().games.iter().map(|g| g.builds.len()).sum()
}

#[cfg(test)]
pub fn cover_images() -> Vec<String> {
    db().games
        .iter()
        .filter_map(|g| g.image.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_product_has_a_release_date() {
        for product in &db().games {
            let date = product.release.as_deref().unwrap_or_else(|| panic!("{} has no release date", product.name));
            let parts: Vec<&str> = date.split('-').collect();
            let valid = parts.len() == 3
                && [4, 2, 2].iter().zip(&parts).all(|(len, part)| part.len() == *len && part.parse::<u32>().is_ok());
            assert!(valid, "{}: {date} is not YYYY-MM-DD", product.name);
        }
    }

    #[test]
    fn release_order_finds_a_game_by_crc_or_by_game_id_and_version() {
        let by_crc = GameRecord {
            crc: Some("0x1fd6136a".into()),
            ..crate::games::test_input::record(Loader::Mice, None)
        };
        assert_eq!(release_order(&by_crc).map(|(date, _)| date), Some("2014-07-17"));

        // A game added from its ICF has a game id and version, and no CRC.
        let from_icf = GameRecord {
            crc: None,
            version: Some("1.70.00".into()),
            ..crate::games::test_input::record(Loader::Segatools, Some("SDGT"))
        };
        assert_eq!(release_order(&from_icf).map(|(date, _)| date), Some("2022-03-17"));

        let unknown = GameRecord { crc: None, version: None, ..crate::games::test_input::record(Loader::Kitten, None) };
        assert_eq!(release_order(&unknown), None);
    }

    #[test]
    fn lookup_mice_d8_returns_product_and_build() {

        let m = lookup(0x1FD6_136A, Loader::Mice).expect("D8 JPN 2.01 should match");
        assert_eq!(m.name, "InitialD Arcade Stage 8 JPN");
        assert_eq!(m.version, "2.01");
        assert_eq!(m.loader, Loader::Mice);
        assert_eq!(m.platform.as_deref(), Some("RingEdge"));
        assert_eq!(m.game_id.as_deref(), Some("SBZZ"));
        assert_eq!(m.sb_crc, Some(0x8667_432A));
        assert_eq!(m.image.as_deref(), Some("logo_D8.png"));
    }

    #[test]
    fn lookup_kitten_d4_returns_product_and_build() {

        let m = lookup(0x361D_1CBE, Loader::Kitten).expect("D4 REV A should match");
        assert_eq!(m.name, "InitialD Arcade Stage 4 JPN");
        assert_eq!(m.version, "REV A");
        assert_eq!(m.loader, Loader::Kitten);
        assert_eq!(m.platform.as_deref(), Some("Lindbergh"));
        assert_eq!(m.sb_crc, Some(0x6049_6EEA));
        assert_eq!(m.image.as_deref(), Some("logo_D4.png"));
    }

    #[test]
    fn lookup_empty_sb_crc_yields_none() {

        let m = lookup(0xD491_0E75, Loader::Kitten).expect("D5 EXP2.0 should match");
        assert_eq!(m.name, "InitialD Arcade Stage 5 EXP 2.0");
        assert_eq!(m.version, "REV A");
        assert_eq!(m.sb_crc, None);
    }

    #[test]
    fn lookup_miss_and_cross_loader() {
        assert!(lookup(0xDEAD_BEEF, Loader::Mice).is_none());

        assert!(lookup(0x361D_1CBE, Loader::Mice).is_none());
        assert!(lookup(0x1FD6_136A, Loader::Kitten).is_none());
    }

    #[test]
    fn card_reader_matches() {
        assert_eq!(match_card_reader(0x037F_F93C).as_deref(), Some("2014"));
        assert_eq!(match_card_reader(0xD293_9F28).as_deref(), Some("2007"));

        assert_eq!(match_card_reader(0x1FD6_136A), None);
    }

    #[test]
    fn all_db_entries_parse_and_loader_match() {

        assert!(
            build_count() >= 16,
            "unexpected number of builds in games_db: {}",
            build_count()
        );
    }

    #[test]
    fn segatools_loader_is_accepted() {
        assert_eq!(parse_loader("segatools"), Some(Loader::Segatools));

        assert_eq!(parse_loader(" SegaTools "), Some(Loader::Segatools));
    }

    #[test]
    fn parse_version_formats() {
        assert_eq!(parse_version("3.21.00"), Some((3, 21, 0)));
        assert_eq!(parse_version(" 12.03.07 "), Some((12, 3, 7)));

        assert_eq!(parse_version("3.21.0"), Some((3, 21, 0)));
        assert_eq!(parse_version("3.21"), Some((3, 21, 0)));

        assert_eq!(parse_version("REV A"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn lookup_by_game_id_hits_d8_build() {

        let m = lookup_by_game_id("SBZZ", "2.01").expect("SBZZ 2.01 should match");
        assert_eq!(m.name, "InitialD Arcade Stage 8 JPN");
        assert_eq!(m.version, "2.01");
        assert_eq!(m.sb_crc, Some(0x8667_432A));
        assert_eq!(m.loader, Loader::Mice);

        let m2 = lookup_by_game_id("SBZZ", "2.01.0").expect("xx.xx and xx.xx.xx should match alike");
        assert_eq!(m2.version, "2.01");

        assert_eq!(lookup_by_game_id("SBZZ", "1.0").unwrap().version, "1.0");
    }

    #[test]
    fn lookup_by_game_id_misses() {
        assert!(lookup_by_game_id("SBZZ", "9.99.99").is_none(), "no such version");
        assert!(lookup_by_game_id("ZZZZ", "1.0").is_none(), "no such product");

        assert!(lookup_by_game_id("SBML", "REV A").is_none());
    }

    #[test]
    fn builds_without_crc_are_tolerated() {

        let raw: RawDb = serde_json::from_str(
            r#"{"games":[{"name":"InitialD The Arcade","game_id":"SDGT",
                 "platform":"ALLS","loader":"segatools",
                 "support_game":[{"version":"3.21.00"},{"crc":"0x00000000","version":"3.00.00"}]}]}"#,
        )
        .expect("an entry without crc should parse");
        let parsed = raw.into_db();
        let g = &parsed.games[0];
        assert_eq!(g.loader, Loader::Segatools);
        assert_eq!(g.builds[0].crc, None, "a missing crc is read as None");
        assert_eq!(g.builds[0].version, "3.21.00");
        assert_eq!(g.builds[1].crc, Some(0), "an explicit 0x00000000 is still valid");
    }
}
