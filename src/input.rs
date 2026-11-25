//! Keys mapped to actions, in one table. The window turns key presses into
//! actions here, and the help screen is built from the same table, so the two
//! can't disagree. Mouse buttons and the wheel are handled in main.rs.

use crate::modes::{Allowed, ModeId};
use crate::player::MoveInput;
use winit::keyboard::KeyCode;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    // Held: true while the key is down (MoveInput).
    Forward,
    Back,
    Left,
    Right,
    Jump,
    Sprint,
    Down,
    Hook,
    LookAround,
    // Pressed: once per press.
    Help,
    Worlds,
    Escape,
    Bike,
    Kayak,
    /// U: get in or out of the world's vehicle; held while driving, reset it.
    Vehicle,
    Glide,
    Camera,
    FreeCamera,
    Screenshot,
    TakeScreenshot,
    Photography,
    Logbook,
    /// Tab: cycle the camera's frame shape.
    Frame,
    Reload,
    /// E: tilt the screenshot camera; held, focus the camera further.
    TiltRight,
    /// Q: tilt the screenshot camera the other way; held, focus nearer.
    TiltLeft,
    Shadows,
    Slopes,
    NextHour,
    /// M: the next kind of weather, which also pins it (world/weather.rs).
    NextWeather,
    /// Y: the next part of the year, which also pins it (world/season.rs).
    NextSeason,
    Style,
    Stats,
    /// The debug minimap, under the stats panel.
    Minimap,
    /// The LOOK page: the light, the distance ramp and the grade.
    Look,
    /// The EDIT page: a creature or plant in a studio, with its library entry
    /// beside it (src/editor).
    Edit,
    Save,
    Fullscreen,
    Select(u8),
    /// Backquote: the animal debug view on or off (game/eco.rs). The five
    /// after it act only while it is on.
    EcoDebug,
    /// P: the ecology map, every animal in the world.
    EcoMap,
    /// I: pin the animal under the crosshair for the inspector.
    Pin,
    /// O: freeze the animals; `.` runs one think of them, `,` ten times as fast.
    Freeze,
    EcoStep,
    EcoSpeed,
    /// `[` and `]` with the ecology map open: the layer before or after.
    EcoLayerBack,
    EcoLayerOn,
}

pub struct Binding {
    pub keys: &'static [KeyCode],
    /// How the help screen names the keys.
    pub name: &'static str,
    pub action: Action,
}

use KeyCode as K;

pub const BINDINGS: &[Binding] = &[
    Binding {
        keys: &[K::KeyW, K::ArrowUp],
        name: "W",
        action: Action::Forward,
    },
    Binding {
        keys: &[K::KeyS, K::ArrowDown],
        name: "S",
        action: Action::Back,
    },
    Binding {
        keys: &[K::KeyA, K::ArrowLeft],
        name: "A",
        action: Action::Left,
    },
    Binding {
        keys: &[K::KeyD, K::ArrowRight],
        name: "D",
        action: Action::Right,
    },
    Binding {
        keys: &[K::Space],
        name: "SPACE",
        action: Action::Jump,
    },
    Binding {
        keys: &[K::ShiftLeft, K::ShiftRight],
        name: "SHIFT",
        action: Action::Sprint,
    },
    Binding {
        keys: &[K::ControlLeft],
        name: "CTRL",
        action: Action::Down,
    },
    Binding {
        keys: &[K::KeyZ],
        name: "Z",
        action: Action::Hook,
    },
    Binding {
        keys: &[K::AltLeft, K::AltRight],
        name: "ALT",
        action: Action::LookAround,
    },
    Binding {
        keys: &[K::KeyH, K::F1],
        name: "H",
        action: Action::Help,
    },
    Binding {
        keys: &[K::KeyL],
        name: "L",
        action: Action::Worlds,
    },
    Binding {
        keys: &[K::Escape],
        name: "ESC",
        action: Action::Escape,
    },
    Binding {
        keys: &[K::KeyX],
        name: "X",
        action: Action::Bike,
    },
    Binding {
        keys: &[K::KeyK],
        name: "K",
        action: Action::Kayak,
    },
    Binding {
        keys: &[K::KeyU],
        name: "U",
        action: Action::Vehicle,
    },
    Binding {
        keys: &[K::KeyC],
        name: "C",
        action: Action::Glide,
    },
    Binding {
        keys: &[K::KeyV],
        name: "V",
        action: Action::Camera,
    },
    Binding {
        keys: &[K::F4],
        name: "F4",
        action: Action::FreeCamera,
    },
    Binding {
        keys: &[K::KeyF],
        name: "F",
        action: Action::Photography,
    },
    Binding {
        keys: &[K::KeyJ],
        name: "J",
        action: Action::Logbook,
    },
    Binding {
        keys: &[K::Tab],
        name: "TAB",
        action: Action::Frame,
    },
    Binding {
        keys: &[K::KeyR],
        name: "R",
        action: Action::Reload,
    },
    Binding {
        keys: &[K::F2],
        name: "F2",
        action: Action::Screenshot,
    },
    Binding {
        keys: &[K::F12, K::Enter, K::NumpadEnter],
        name: "F12",
        action: Action::TakeScreenshot,
    },
    Binding {
        keys: &[K::KeyE],
        name: "E",
        action: Action::TiltRight,
    },
    Binding {
        keys: &[K::KeyQ],
        name: "Q",
        action: Action::TiltLeft,
    },
    Binding {
        keys: &[K::KeyG],
        name: "G",
        action: Action::Shadows,
    },
    Binding {
        keys: &[K::KeyB],
        name: "B",
        action: Action::Slopes,
    },
    Binding {
        keys: &[K::KeyN],
        name: "N",
        action: Action::NextHour,
    },
    Binding {
        keys: &[K::KeyM],
        name: "M",
        action: Action::NextWeather,
    },
    Binding {
        keys: &[K::KeyY],
        name: "Y",
        action: Action::NextSeason,
    },
    Binding {
        keys: &[K::KeyT],
        name: "T",
        action: Action::Style,
    },
    Binding {
        keys: &[K::F3],
        name: "F3",
        action: Action::Stats,
    },
    Binding {
        keys: &[K::F6],
        name: "F6",
        action: Action::Minimap,
    },
    Binding {
        keys: &[K::F7],
        name: "F7",
        action: Action::Look,
    },
    Binding {
        keys: &[K::F8],
        name: "F8",
        action: Action::Edit,
    },
    Binding {
        keys: &[K::F5],
        name: "F5",
        action: Action::Save,
    },
    Binding {
        keys: &[K::F11],
        name: "F11",
        action: Action::Fullscreen,
    },
    Binding {
        keys: &[K::Backquote],
        name: "`",
        action: Action::EcoDebug,
    },
    Binding {
        keys: &[K::KeyP],
        name: "P",
        action: Action::EcoMap,
    },
    Binding {
        keys: &[K::KeyI],
        name: "I",
        action: Action::Pin,
    },
    Binding {
        keys: &[K::KeyO],
        name: "O",
        action: Action::Freeze,
    },
    Binding {
        keys: &[K::Period],
        name: ".",
        action: Action::EcoStep,
    },
    Binding {
        keys: &[K::Comma],
        name: ",",
        action: Action::EcoSpeed,
    },
    Binding {
        keys: &[K::BracketLeft],
        name: "[",
        action: Action::EcoLayerBack,
    },
    Binding {
        keys: &[K::BracketRight],
        name: "]",
        action: Action::EcoLayerOn,
    },
    Binding {
        keys: &[K::Digit1],
        name: "1",
        action: Action::Select(0),
    },
    Binding {
        keys: &[K::Digit2],
        name: "2",
        action: Action::Select(1),
    },
    Binding {
        keys: &[K::Digit3],
        name: "3",
        action: Action::Select(2),
    },
    Binding {
        keys: &[K::Digit4],
        name: "4",
        action: Action::Select(3),
    },
    Binding {
        keys: &[K::Digit5],
        name: "5",
        action: Action::Select(4),
    },
    Binding {
        keys: &[K::Digit6],
        name: "6",
        action: Action::Select(5),
    },
    Binding {
        keys: &[K::Digit7],
        name: "7",
        action: Action::Select(6),
    },
    Binding {
        keys: &[K::Digit8],
        name: "8",
        action: Action::Select(7),
    },
    Binding {
        keys: &[K::Digit9],
        name: "9",
        action: Action::Select(8),
    },
    // The tenth slot, which is what 0 has picked in every arena shooter
    // since there were ten guns to pick from.
    Binding {
        keys: &[K::Digit0],
        name: "0",
        action: Action::Select(9),
    },
];

/// The action a key is bound to.
pub fn action(code: KeyCode) -> Option<Action> {
    BINDINGS.iter().find(|b| b.keys.contains(&code)).map(|b| b.action)
}

/// Set a held action in the movement input; false if the action isn't held.
/// Q and E are pressed actions that are also tracked as held, for the
/// camera's manual focus, so they return false.
pub fn hold(i: &mut MoveInput, a: Action, down: bool) -> bool {
    match a {
        Action::TiltLeft => i.focus_near = down,
        Action::TiltRight => i.focus_far = down,
        // Pressed (main.rs posts the press) and held (the hold resets the car).
        Action::Vehicle => i.vehicle = down,
        _ => {}
    }
    let slot = match a {
        Action::Forward => &mut i.forward,
        Action::Back => &mut i.back,
        Action::Left => &mut i.left,
        Action::Right => &mut i.right,
        Action::Jump => &mut i.jump,
        Action::Sprint => &mut i.sprint,
        Action::Down => &mut i.down,
        Action::Hook => &mut i.hook,
        Action::LookAround => &mut i.look,
        _ => return false,
    };
    *slot = down;
    true
}

/// One line of the help screen: the actions it names (their keys are looked
/// up in BINDINGS unless `keys` says otherwise), what they do, and the mode
/// it belongs to, so a world without that mode leaves the line out.
pub struct HelpRow {
    pub actions: &'static [Action],
    pub keys: Option<&'static str>,
    pub text: &'static str,
    pub mode: Option<ModeId>,
}

const fn row(actions: &'static [Action], text: &'static str) -> HelpRow {
    HelpRow {
        actions,
        keys: None,
        text,
        mode: None,
    }
}

/// A box on the help screen: a set of controls under a heading. A box whose
/// rows all belong to modes a world leaves out isn't drawn at all.
pub struct HelpBox {
    pub title: &'static str,
    pub rows: &'static [HelpRow],
}

const fn keys(k: &'static str, text: &'static str, mode: Option<ModeId>) -> HelpRow {
    HelpRow {
        actions: &[],
        keys: Some(k),
        text,
        mode,
    }
}

const fn acts(a: &'static [Action], text: &'static str, mode: ModeId) -> HelpRow {
    HelpRow {
        actions: a,
        keys: None,
        text,
        mode: Some(mode),
    }
}

pub const HELP: &[HelpBox] = &[
    HelpBox {
        title: "THE WORLD",
        rows: &[
            row(&[Action::Worlds], "CHOOSE A WORLD"),
            row(&[Action::Shadows, Action::Style, Action::Slopes], "SHADOWS / STYLE / SLOPES"),
            row(&[Action::NextHour], "THE NEXT HOUR OF THE DAY"),
            row(&[Action::NextWeather], "THE NEXT KIND OF WEATHER"),
            row(&[Action::NextSeason], "THE NEXT SEASON"),
            row(&[Action::Look], "LOOK AND COLOUR: THE RAMP AND THE GRADE"),
            row(&[Action::Edit], "EDIT A CREATURE OR PLANT IN A STUDIO"),
            HelpRow {
                actions: &[Action::Screenshot, Action::TakeScreenshot],
                keys: None,
                text: "SCREENSHOT MODE / QUICK SCREENSHOT",
                mode: Some(ModeId::Screenshot),
            },
            row(&[Action::Stats, Action::Minimap], "STATS / DEBUG MINIMAP"),
            row(&[Action::FreeCamera, Action::Save, Action::Fullscreen], "FREE CAMERA / SAVE / FULLSCREEN"),
            row(&[Action::Escape], "PAUSE MENU"),
        ],
    },
    HelpBox {
        title: "ON FOOT",
        rows: &[
            keys("WASD", "MOVE", None),
            keys("MOUSE", "LOOK", None),
            row(&[Action::Jump], "JUMP"),
            row(&[Action::Sprint], "SPRINT"),
            row(&[Action::Down], "CROUCH: LOWER, SLOWER, LESS ALARMING"),
            keys("WALK INTO A WALL", "CLIMB", Some(ModeId::Climb)),
            keys("LEFT / RIGHT CLICK", "DIG / BUILD", None),
            keys("1-9 / WHEEL", "BUILDING MATERIAL / BRUSH SIZE", None),
        ],
    },
    HelpBox {
        title: "CAMERA",
        rows: &[
            acts(&[Action::Photography], "TAKE IT OUT, PUT IT AWAY", ModeId::Camera),
            keys("CLICK", "RAISE THE VIEWFINDER, THEN THE SHUTTER", Some(ModeId::Camera)),
            keys("RIGHT CLICK", "LOWER THE VIEWFINDER", Some(ModeId::Camera)),
            acts(&[Action::Frame], "FRAME SHAPE: LANDSCAPE, PORTRAIT, SQUARE", ModeId::Camera),
            keys("WHEEL / SHIFT+WHEEL", "ZOOM / APERTURE", Some(ModeId::Camera)),
            keys("CTRL+WHEEL / Q E", "FOCUS BY HAND", Some(ModeId::Camera)),
            keys("MIDDLE CLICK", "BACK ON AUTOFOCUS", Some(ModeId::Camera)),
            acts(&[Action::Logbook], "LOGBOOK AND SCRAPBOOK", ModeId::Camera),
            acts(&[Action::Reload], "LOAD A ROLL OF FILM", ModeId::Camera),
        ],
    },
    HelpBox {
        title: "WINGSUIT",
        rows: &[
            acts(&[Action::Glide], "LAUNCH, OPEN OR FOLD THE WINGS", ModeId::Glide),
            keys("MOUSE", "ROLL INTO A TURN", Some(ModeId::Glide)),
            acts(&[Action::LookAround], "LOOK AROUND WITHOUT STEERING", ModeId::Glide),
            acts(&[Action::Camera], "FIRST PERSON OR CHASE CAMERA", ModeId::Glide),
        ],
    },
    HelpBox {
        title: "BIKE",
        rows: &[
            acts(&[Action::Bike], "GET ON, GET OFF", ModeId::Bike),
            acts(&[Action::Jump], "LOAD THE SUSPENSION, LET GO TO POP", ModeId::Bike),
            acts(&[Action::Down], "TUCK: LOWER DRAG, A HARDER PULL DOWNHILL", ModeId::Bike),
            acts(&[Action::Camera], "FIRST PERSON OR CHASE CAMERA", ModeId::Bike),
        ],
    },
    HelpBox {
        title: "DRIVING",
        rows: &[
            acts(&[Action::Vehicle], "GET IN, GET OUT; FAR FROM IT, BRING IT OVER", ModeId::Drive),
            keys("HOLD U", "PUT IT BACK ON ITS WHEELS", Some(ModeId::Drive)),
            keys("W / S", "THROTTLE / BRAKE, THEN REVERSE", Some(ModeId::Drive)),
            keys("MOUSE / A D", "STEER WHERE YOU LOOK", Some(ModeId::Drive)),
            acts(&[Action::Down], "HANDBRAKE", ModeId::Drive),
            acts(&[Action::Jump], "LOAD THE SPRINGS, LET GO TO HOP", ModeId::Drive),
            acts(&[Action::Sprint], "BOOST", ModeId::Drive),
            keys("W S A D IN THE AIR", "PITCH AND ROLL", Some(ModeId::Drive)),
            acts(&[Action::Camera], "CHASE OR FIRST PERSON", ModeId::Drive),
        ],
    },
    HelpBox {
        title: "KAYAK",
        rows: &[
            acts(&[Action::Kayak], "GET IN AT THE WATER, GET OUT", ModeId::Kayak),
            keys("K IN DEEP WATER", "OVER THE SIDE; K BESIDE IT TO CLIMB BACK IN", Some(ModeId::Kayak)),
        ],
    },
    HelpBox {
        title: "SCUBA",
        rows: &[
            keys("CTRL IN DEEP WATER", "DIVE", None),
            keys("WASD / MOUSE", "SWIM WHERE YOU LOOK", None),
            keys("SPACE / CTRL", "UP / DOWN", None),
            keys("SHIFT", "KICK; IN TIME WITH THE LAST, A STRONGER ONE", None),
            keys("HOLD E", "HOLD ON TO A WHALE, MANTA OR TURTLE", None),
            keys("60 SECONDS", "OF BREATH, WHOLE AGAIN AT THE SURFACE", None),
        ],
    },
    HelpBox {
        title: "CONTROLLER",
        rows: &[
            keys("LEFT STICK / RIGHT STICK", "MOVE OR STEER / LOOK", None),
            keys("RT / LT", "THROTTLE / BRAKE; ON FOOT DIG / BUILD, FIRE", None),
            keys("A OR RB", "JUMP; IN THE CAR HOP, HELD TO DRIFT", None),
            keys("B / LB", "CROUCH, HANDBRAKE / SPRINT, BOOST", None),
            keys("X / Y", "CAR IN OR OUT, HELD: RESET / CAMERA", None),
            keys("D-PAD", "GLIDE / KAYAK / BIKE / CAMERA OUT", None),
            keys("START / BACK", "PAUSE / HELP", None),
        ],
    },
    HelpBox {
        title: "ANIMALS (DEBUG)",
        rows: &[
            row(&[Action::EcoDebug], "ANIMAL DEBUG ON OR OFF; THE KEYS BELOW WORK WHILE IT IS ON"),
            row(&[Action::EcoMap], "ECOLOGY MAP: EVERY ANIMAL BY WHAT IT IS DOING"),
            row(&[Action::EcoLayerBack, Action::EcoLayerOn], "MAP LAYER: THE ANIMALS, OR A CHANNEL OF THE WORLD SURVEY"),
            row(&[Action::Pin], "PIN THE ANIMAL UNDER THE CROSSHAIR: ITS PANEL AND LINES"),
            row(&[Action::Freeze, Action::EcoStep, Action::EcoSpeed], "FREEZE / ONE THINK / TEN TIMES AS FAST"),
        ],
    },
    HelpBox {
        title: "ROPE",
        rows: &[acts(&[Action::Hook], "GET IT OUT, THROW, NEXT ANCHOR, LET GO", ModeId::Rope)],
    },
    HelpBox {
        title: "ARENA",
        rows: &[
            keys("LEFT / RIGHT CLICK", "FIRE / THE OTHER FIRE MODE", Some(ModeId::Shooter)),
            keys("1-9 / WHEEL", "PICK A WEAPON", Some(ModeId::Shooter)),
            acts(&[Action::Reload], "RELOAD", ModeId::Shooter),
            keys("JUMP TWICE", "DOUBLE JUMP", Some(ModeId::Shooter)),
            keys("AIM AT YOUR FEET", "ROCKET JUMP", Some(ModeId::Shooter)),
        ],
    },
];

/// The help screen's boxes for a world allowing `allowed`: a title and its
/// lines (keys, what they do). Empty boxes are left out.
pub fn help_boxes(allowed: Allowed) -> Vec<(&'static str, Vec<(String, &'static str)>)> {
    HELP.iter()
        .map(|b| {
            let rows: Vec<(String, &'static str)> = b
                .rows
                .iter()
                .filter(|r| r.mode.map_or(true, |m| allowed.has(m)))
                .map(|r| {
                    let keys = r.keys.map(str::to_string).unwrap_or_else(|| {
                        r.actions
                            .iter()
                            .filter_map(|a| BINDINGS.iter().find(|b| b.action == *a).map(|b| b.name))
                            .collect::<Vec<_>>()
                            .join(" / ")
                    });
                    (keys, r.text)
                })
                .collect();
            (b.title, rows)
        })
        .filter(|(_, rows)| !rows.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No key is bound twice, and every help line's actions have a key.
    #[test]
    fn bindings_are_unique_and_help_lines_resolve() {
        let mut seen = Vec::new();
        for b in BINDINGS {
            for k in b.keys {
                assert!(!seen.contains(k), "{k:?} bound twice");
                seen.push(*k);
            }
        }
        for bx in HELP {
            for r in bx.rows {
                for a in r.actions {
                    assert!(BINDINGS.iter().any(|b| b.action == *a), "{a:?} has no key");
                }
            }
        }
        let boxes = help_boxes(Allowed::all());
        let rows: Vec<(String, &str)> = boxes.iter().flat_map(|(_, r)| r.clone()).collect();
        assert!(rows.iter().any(|(k, t)| k == "G / T / B" && *t == "SHADOWS / STYLE / SLOPES"));
        assert!(rows.iter().any(|(k, t)| k == "F3 / F6" && *t == "STATS / DEBUG MINIMAP"));
        assert!(boxes.iter().any(|(t, _)| *t == "CAMERA") && boxes.iter().any(|(t, _)| *t == "BIKE"));
        let walk_only = help_boxes(Allowed::from_names(&[], |_| {}));
        assert!(!walk_only.iter().any(|(t, _)| *t == "BIKE"), "a walking world has no bike box");
        assert!(walk_only.iter().any(|(t, _)| *t == "ON FOOT"));
    }
}
