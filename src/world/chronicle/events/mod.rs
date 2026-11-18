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
    passage: Passage,
    tower: TowerEvent,
    erode_rims: ErodeRims,
    path: Path,
    moisture: Moisture,
    canyon: Canyon,
    vale: Vale,
    mesa: Mesa,
    road: Road,
    track: Track,
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
