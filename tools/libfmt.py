"""Format the shared library and write its catalogue.

    python tools/libfmt.py            reformat every library/*/*.json in place
    python tools/libfmt.py --check    list files that are not in canonical form
    python tools/libfmt.py --catalogue  also rewrite library/CATALOGUE.md
                                        (--catalog, the binary's spelling, works too)

Canonical form: identity keys first (name, version, introduced_by, notes), then
the entry's own keys in a fixed order, one key per line; short values (numbers,
colour triples, small objects) stay on one line, and lists of ops or uses get
one item per line. Formatting only trims floats to six significant digits (float noise from
scripts); the file is parsed and compared before it is written.

The catalogue lists every entry by kind with who introduced it, its notes, and
which worlds use it, by the engine's own rule (`worldload::reach`): any
string in a world's world.json, chronicle.json and the data files they name
that is an entry's name reaches it, and so does any string in the files of the
biomes, plants, structures, signs, kits and cities reached. It reads `worlds/`
and `local/worlds/` alike (never naming the local ones), and counts the materials
`worldload::ENGINE_MATERIALS` names in Rust and the shooter's pad platforms
as reached, so "unused" means no world can get at the entry and it is safe to
delete.
"""

import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LIB = os.path.join(ROOT, "library")
KINDS = ["materials", "plants", "structures", "kits", "cities", "creatures", "collectibles", "biomes", "habitats", "realms", "signs", "weapons", "fx"]
WIDTH = 118

FIRST = ["name", "version", "introduced_by", "_generated_by", "notes"]
ORDER = {
    "materials": ["like", "shade", "hue", "was", "id", "smooth", "crisp", "passable", "slab", "water", "glow", "glints", "foliage", "surface", "tint", "drift", "sprites", "texture", "top", "bottom"],
    "plants": ["form", "height", "trunk_radius", "buttress", "crown_radius", "bark", "leaf", "leaf_alt", "morphs", "morph_blob_m", "squash", "curve", "canopy", "leader", "fill", "spread", "dome", "limb", "max_slope"],
    "structures": ["size", "anchor", "foundation", "level", "decays", "base", "vines", "tags", "slots", "ops"],
    "kits": ["ground", "width", "mat", "shoulder", "margin", "lift", "sag", "clear", "max_span", "walk", "deck", "rails", "posts", "piers", "braces", "span"],
    "cities": ["like", "road", "pavement", "glass", "metal", "deck", "deck_top", "rail", "chimney", "porch_floor", "doors", "plinths", "ivy", "ivy_mat", "tree_boost", "remap"],
    "creatures": ["voxel", "grid", "half_w", "height", "colors", "morphs", "forms", "behaviour", "in_water", "ops"],
    "collectibles": ["display", "voxel", "spin", "colors", "ops"],
    "habitats": ["wet", "soil", "shade", "rock", "disturbed", "open", "forage"],
    "biomes": ["habitat", "priority", "rules", "surface", "subsurface", "rock", "fringe", "soil", "map_color", "plants", "features", "creatures", "collectibles", "sound"],
    "signs": ["post", "board", "centre_m", "offset_m", "thickness", "face", "edge", "ink", "cap_m", "border_m", "letters", "two_sided", "bare"],
    "weapons": ["display", "slot", "ammo_max", "ammo_start", "mag", "reload_s", "zoom", "fire", "alt", "dig", "view", "grip", "tint"],
    "fx": ["shape", "count", "speed", "cone_deg", "life", "size", "size_jitter", "gravity", "drag", "wind", "settle", "flutter", "color",
           "material", "alpha", "glow", "near_fade"],
}


def one_line(v):
    return json.dumps(v, separators=(", ", ": "), ensure_ascii=False)


def fmt(v, indent):
    """Value at `indent` spaces: on one line if it fits, else broken up."""
    s = one_line(v)
    if len(s) + indent <= WIDTH or not isinstance(v, (list, dict)) or not v:
        return s
    pad = " " * (indent + 2)
    if isinstance(v, list):
        items = [pad + fmt(x, indent + 2) for x in v]
        return "[\n" + ",\n".join(items) + "\n" + " " * indent + "]"
    items = [pad + json.dumps(k) + ": " + fmt(x, indent + 2) for k, x in v.items()]
    return "{\n" + ",\n".join(items) + "\n" + " " * indent + "}"


def ordered(d, kind):
    keys = [k for k in FIRST if k in d] + [k for k in ORDER.get(kind, []) if k in d]
    keys += [k for k in d if k not in keys]
    return {k: d[k] for k in keys}


def tidy(v):
    """Strip float noise from scripted shape programs (9.799999999999999 ->
    9.8) by keeping six significant digits. Never round to a fixed number of
    decimals: densities like 0.00015 are real values."""
    if isinstance(v, float):
        return float("%.6g" % v)
    if isinstance(v, list):
        return [tidy(x) for x in v]
    if isinstance(v, dict):
        return {k: tidy(x) for k, x in v.items()}
    return v


def close(a, b):
    if isinstance(a, (int, float)) and isinstance(b, (int, float)):
        return abs(a - b) <= 1e-5 * max(abs(a), abs(b))
    if isinstance(a, list) and isinstance(b, list):
        return len(a) == len(b) and all(close(x, y) for x, y in zip(a, b))
    if isinstance(a, dict) and isinstance(b, dict):
        return a.keys() == b.keys() and all(close(a[k], b[k]) for k in a)
    return a == b


def render(d, kind):
    d = tidy(ordered(d, kind))
    lines = []
    for k, v in d.items():
        key = json.dumps(k)
        lines.append("  " + key + ": " + fmt(v, 2))
    return "{\n" + ",\n".join(lines) + "\n}\n"


_UNTRACKED = None


def untracked():
    """Library files git ignores, which CATALOGUE.md -- itself published --
    therefore leaves out. Some entries are written by a tool out of data that
    is not ours to ship (third_party/README.md), and .gitignore is the one
    place that says which. Empty if there is no git here, which only means a
    checkout without one lists everything."""
    global _UNTRACKED
    if _UNTRACKED is None:
        cmd = ["git", "-C", ROOT, "ls-files", "--others", "--ignored", "--exclude-standard", "--", "library"]
        try:
            out = subprocess.run(cmd, capture_output=True, text=True, check=True).stdout
            _UNTRACKED = {os.path.normpath(os.path.join(ROOT, line)) for line in out.splitlines() if line}
        except (OSError, subprocess.SubprocessError):
            _UNTRACKED = set()
    return _UNTRACKED


def files():
    for kind in KINDS:
        folder = os.path.join(LIB, kind)
        if not os.path.isdir(folder):
            continue
        skip = untracked()
        for f in sorted(os.listdir(folder)):
            path = os.path.join(folder, f)
            if f.endswith(".json") and os.path.normpath(path) not in skip:
                yield kind, path


def load_all():
    lib = {k: {} for k in KINDS}
    for kind, path in files():
        with open(path, encoding="utf-8") as fh:
            d = json.load(fh)
        lib[kind][d.get("name", os.path.basename(path)[:-5])] = d
    return lib


def engine_materials():
    """The materials the engine names in its own code, read from the Rust so
    the two lists cannot drift (`worldload::ENGINE_MATERIALS`). Every world
    reaches these, whatever its biomes say."""
    path = os.path.join(ROOT, "src", "worldload.rs")
    try:
        with open(path, encoding="utf-8") as fh:
            src = fh.read()
    except OSError:
        return []
    # The definition, not the first mention (a comment above it names it too).
    i = src.find("const ENGINE_MATERIALS")
    if i < 0:
        return []
    # After the `=`: the type, `[&str; 19]`, has brackets of its own.
    eq = src.find("=", i)
    start, end = src.find("[", eq), src.find("]", eq)
    if start < 0 or end < 0:
        return []
    return [s.strip().strip('"') for s in src[start + 1 : end].split(",") if s.strip()]


def world_dirs():
    """Every world folder, as (label, path). `local/worlds` is kept out of the
    published game (local/README.md) but its worlds reach library entries just
    the same, so leaving it out would report those entries unused. They all
    answer to the one label "local", which the catalogue never prints, since
    CATALOGUE.md is published and an unpublished world is not."""
    out = []
    for root, published in [(os.path.join(ROOT, "worlds"), True), (os.path.join(ROOT, "local", "worlds"), False)]:
        for w in sorted(os.listdir(root)) if os.path.isdir(root) else []:
            if os.path.isdir(os.path.join(root, w)):
                out.append((w if published else "local", os.path.join(root, w)))
    return out


def strings(v, out):
    """Every string in a JSON value but its notes, as `worldload::reach` reads
    them (it reads notes too; prose here never names an entry exactly)."""
    if isinstance(v, str):
        out.append(v)
    elif isinstance(v, list):
        for x in v:
            strings(x, out)
    elif isinstance(v, dict):
        for k, x in v.items():
            if k != "notes":
                strings(x, out)


def read_json(path):
    try:
        with open(path, encoding="utf-8") as fh:
            return json.load(fh)
    except (OSError, ValueError):
        return None


def pad_material(p):
    """The platform the shooter stamps under a pickup (`PickupDef::pad_material`)."""
    kind = p.get("kind", "")
    if kind == "health":
        return "pickup_pad_health"
    if kind in ("armour", "armor"):
        return "pickup_pad_armour"
    if kind in ("ammo", "weapon") and p.get("weapon"):
        return "weapon_pad_" + p["weapon"]
    return None


# The kinds whose files can name materials or other entries, as in
# `worldload::reach`. Creatures are left out on purpose: a creature's `hunts`
# names prey that only matters where it already lives.
WALKED = ["biomes", "plants", "structures", "signs", "kits", "cities"]


def local_world_names():
    """The folder names under `local/worlds`, which the catalogue keeps out of
    its "from" column as well."""
    root = os.path.join(ROOT, "local", "worlds")
    return {w for w in os.listdir(root) if os.path.isdir(os.path.join(root, w))} if os.path.isdir(root) else set()


def world_uses(lib):
    """{(kind, name): [world, ...]} for every entry a world reaches. The rule
    is the engine's (`worldload::reach`): any string in the world's
    world.json and chronicle.json, and in any .json data file they name, that
    is the name of a library entry reaches it, and so does any string in the
    files of the entries reached, and so on. A list of keys to look under
    would drift from the engine every time an event takes a new param."""
    uses = {}
    engine = engine_materials()
    by_name = {}
    for kind in KINDS:
        for n in lib[kind]:
            by_name.setdefault(n, []).append(kind)
    for wname, wpath in world_dirs():
        wd = read_json(os.path.join(wpath, "world.json"))
        ch = read_json(os.path.join(wpath, "chronicle.json"))
        if wd is None or ch is None:
            continue
        todo = list(engine)
        strings(wd, todo)
        strings(ch, todo)
        for p in (wd.get("shooter") or {}).get("pickups", []):
            m = pad_material(p)
            if m:
                todo.append(m)
        if "biomes" not in wd:
            todo.extend(lib["biomes"])
        for f in list(todo):
            if f.endswith(".json"):
                d = read_json(os.path.join(wpath, f))
                if d is not None:
                    strings(d, todo)
        seen = set()
        hit = {("fx", n) for n in lib["fx"]}
        while todo:
            n = todo.pop()
            if n in seen:
                continue
            seen.add(n)
            for kind in by_name.get(n, []):
                hit.add((kind, n))
                if kind in WALKED:
                    strings(lib[kind][n], todo)
                elif kind == "collectibles":
                    todo.append(n + "_mother")
            # A merged material answers to its old names (`was`).
            for m, d in lib["materials"].items():
                if n in d.get("was", []):
                    hit.add(("materials", m))
        # A creature brings in its parents by `like`, and a rare morph comes
        # with its parent (a morph of a morph too), as worldload does.
        for _ in range(4):
            for name, c in lib["creatures"].items():
                parent = c.get("like")
                if ("creatures", name) in hit and parent in lib["creatures"]:
                    hit.add(("creatures", parent))
                if c.get("rarity", 0) > 0 and parent and ("creatures", parent) in hit:
                    hit.add(("creatures", name))
                # A form drawn with another entry's model (the lioness).
                if ("creatures", name) in hit:
                    for f in (c.get("forms") or {}).values():
                        if f in lib["creatures"]:
                            hit.add(("creatures", f))
        for key in hit:
            uses.setdefault(key, []).append(wname)
    return uses


def catalogue(lib):
    uses = world_uses(lib)
    unshipped = local_world_names() | {"local"}
    out = [
        "# Library catalogue",
        "",
        "Generated by `python tools/libfmt.py --catalogue`; do not edit by hand.",
        "Every entry, who introduced it, and the worlds that reach it (through their",
        "biome list, strata, chronicle events and structure slots). An entry no world",
        "reaches is listed as unused, and one reached only by worlds kept out of",
        "the repository as \"no shipped world\".",
        "",
    ]
    for kind in KINDS:
        if not lib[kind]:
            continue
        out.append("## " + kind.capitalize())
        out.append("")
        if kind == "materials":
            out.append("| id | name | from | used by | notes |")
            out.append("|---|---|---|---|---|")
            rows = sorted(lib[kind].values(), key=lambda d: d.get("id", 0))
        else:
            out.append("| name | from | used by | notes |")
            out.append("|---|---|---|---|")
            rows = sorted(lib[kind].values(), key=lambda d: d["name"])
        for d in rows:
            reached = set(uses.get((kind, d["name"]), []))
            u = ", ".join(sorted(reached - {"local"})) or ("no shipped world" if reached else "unused")
            note = (d.get("notes") or "").replace("|", "/").replace("\n", " ")
            # The form or the verb, in front of the notes -- unless the notes
            # already open with it, which many of them do.
            lead = d.get("form", "") if kind == "plants" else d.get("behaviour", {}).get("movement", "") if kind == "creatures" else ""
            if lead and not note.startswith(lead + ":"):
                note = (lead + ": " + note).strip(": ")
            src = d.get("introduced_by", "?")
            cells = [d["name"], "-" if src in unshipped else src, u, note]
            if kind == "materials":
                cells = [str(d.get("id", "?"))] + cells
            out.append("| " + " | ".join(cells) + " |")
        out.append("")
    used_ids = sorted(d["id"] for d in lib["materials"].values() if "id" in d)
    out.append("Material ids asked for: " + ", ".join(str(i) for i in used_ids)
               + ". Materials without one (?) take free ids at load, per world (Library::assign_ids); new materials leave id out.")
    out.append("")
    return "\n".join(out)


def main():
    check = "--check" in sys.argv
    bad = []
    for kind, path in files():
        with open(path, encoding="utf-8") as fh:
            text = fh.read()
        d = json.loads(text)
        new = render(d, kind)
        assert close(json.loads(new), d), path
        if new != text.replace("\r\n", "\n"):
            bad.append(path)
            if not check:
                with open(path, "w", encoding="utf-8", newline="\n") as fh:
                    fh.write(new)
    rel = [os.path.relpath(p, ROOT) for p in bad]
    if check:
        print("\n".join(rel) if rel else "all canonical")
        sys.exit(1 if rel else 0)
    print("reformatted %d files" % len(rel))
    if "--catalogue" in sys.argv or "--catalog" in sys.argv:
        with open(os.path.join(LIB, "CATALOGUE.md"), "w", encoding="utf-8", newline="\n") as fh:
            fh.write(catalogue(load_all()))
        print("wrote library/CATALOGUE.md")


if __name__ == "__main__":
    main()
