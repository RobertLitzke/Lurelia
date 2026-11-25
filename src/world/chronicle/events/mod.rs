//! The event registry. Each event kind is a file here named after its
//! `event` name in chronicle.json, holding a params struct that implements
//! `Process`. To add one, write the file and add a line to `registry!` below.

use super::Process;
use serde::de::DeserializeOwned;

/// One event kind: its name in chronicle.json and how to read its fields.
pub struct EventType {
    pub name: &'static str,
    pub parse: Parse,
}

/// Reads an event's own fields, passing each key it doesn't know to the
/// callback.
pub type Parse = fn(serde_json::Value, &mut dyn FnMut(String)) -> Result<Box<dyn Process>, serde_json::Error>;

fn parse<T: DeserializeOwned + Process + 'static>(v: serde_json::Value, unknown: &mut dyn FnMut(String)) -> Result<Box<dyn Process>, serde_json::Error> {
    Ok(Box::new(crate::library::parse_known::<T>(v, unknown)?))
}

/// `module: Type` per event kind: declares `mod module;` and registers `Type`
/// under the name `module`.
macro_rules! registry {
    ($($name:ident: $ty:ident,)*) => {
        $(pub mod $name;)*
        pub const EVENTS: &[EventType] = &[$(EventType { name: stringify!($name), parse: parse::<$name::$ty> },)*];
    };
}

registry! {
    landmass: Landmass,
    volcano: Volcano,
    ashfall: Ashfall,
    impact: Impact,
    bloom: Bloom,
    realm: RealmEvent,
    river: River,
    lake: Lake,
    avulsion: Avulsion,
    deposit: Deposit,
    settlement: Settlement,
    place: Place,
    abandon: Abandon,
    fire: Fire,
    region: Region,
    escarpment: Escarpment,
    lava_tube: LavaTube,
    find: Find,
    passage: Passage,
    tower: TowerEvent,
    skeleton: Skeleton,
    erode_rims: ErodeRims,
    path: Path,
    moisture: Moisture,
    canyon: Canyon,
    vale: Vale,
    mesa: Mesa,
    road: Road,
    track: Track,
    circuit: Circuit,
    spires: Spires,
    reef: Reef,
    dunes: Dunes,
    heightmap: Heightmap,
    city: City,
    subside: Subside,
    bayou: Bayou,
    earthwork: Earthwork,
    area: AreaEvent,
    sign: SignEvent,
    lamp: LampEvent,
    ribbon: Ribbon,
    route: Route,
    network: Network,
    village: Village,
    town: Town,
    fields: FieldsEvent,
    watercourse: Watercourse,
    trough: Trough,
    cirque: Cirque,
    fell: Fell,
    spring: Spring,
    dip: Dip,
    sea_ice: SeaIce,
    icefall: Icefall,
    coral_reef: CoralReef,
    atoll: Atoll,
    cenote: Cenote,
    vent: Vent,
    terrain: Terrain,
    drainage: Drainage,
    glaciate: Glaciate,
    seat: Seat,
    strandline: Strandline,
    canopy_town: CanopyTown,
    flags: Flags,
    kites: Kites,
    roost: Roost,
    pahoehoe: Pahoehoe,
}

/// The library entries event `e` (its object in chronicle.json) uses without
/// naming them: what its code falls back on for the fields `e` leaves out.
/// No file spells these, so `worldload::reach` adds them to the world's
/// reach. An event that names a library entry in code lists it here.
pub fn defaults_used(e: &serde_json::Value, lib: &crate::library::Library) -> Vec<&'static str> {
    let left_out = |field: &str| e.get(field).is_none_or(|v| v.is_null());
    let town_kit = || e.get("kit").and_then(|k| k.as_str()).and_then(|k| lib.city_kit(k));
    match e.get("event").and_then(|v| v.as_str()).unwrap_or("") {
        "town" | "fields" => town_kit().map(|k| k.town.defaults_used()).unwrap_or_default(),
        "circuit" => circuit::DEFAULTS.to_vec(),
        "atoll" if left_out("rock") => vec![atoll::ROCK],
        "coral_reef" if left_out("rock") => vec![coral_reef::ROCK],
        "sign" if left_out("style") => vec![sign::STYLE],
        "city" if left_out("kit") => vec![city::KIT],
        _ => Vec::new(),
    }
}

pub fn find(name: &str) -> Option<&'static EventType> {
    EVENTS.iter().find(|e| e.name == name)
}

/// The error for an `event` name the registry doesn't have, worded as serde
/// words an unknown enum variant.
pub fn unknown(name: &str) -> String {
    let names: Vec<String> = EVENTS.iter().map(|e| format!("`{}`", e.name)).collect();
    format!("unknown variant `{name}`, expected one of {}", names.join(", "))
}
