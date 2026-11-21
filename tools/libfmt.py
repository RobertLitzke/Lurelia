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
which worlds use it (found by reading each world's world.json and
chronicle.json and following biomes to their plants, features, creatures and
collectibles). It reads `worlds/` and `local/worlds/` alike, and counts the
materials `worldload::ENGINE_MATERIALS` names in Rust as reached by every
world, so "unused" means no world can get at the entry and it is safe to
delete.
"""

import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LIB = os.path.join(ROOT, "library")
KINDS = ["materials", "plants", "structures", "kits", "cities", "creatures", "collectibles", "biomes", "signs", "weapons", "fx"]
WIDTH = 118

FIRST = ["name", "version", "introduced_by", "_generated_by", "notes"]
ORDER = {
    "materials": ["id", "smooth", "crisp", "passable", "slab", "water", "glow", "glints", "foliage", "tint", "drift", "texture", "top", "bottom"],
    "plants": ["form", "height", "trunk_radius", "buttress", "crown_radius", "bark", "leaf", "leaf_alt", "morphs", "morph_blob_m", "squash", "curve", "canopy", "leader", "fill", "spread", "dome", "limb", "max_slope"],
    "structures": ["size", "anchor", "foundation", "level", "decays", "base", "vines", "tags", "slots", "ops"],
    "kits": ["ground", "width", "mat", "shoulder", "margin", "lift", "sag", "clear", "max_span", "walk", "deck", "rails", "posts", "piers", "span"],
    "cities": ["like", "road", "pavement", "glass", "metal", "deck", "deck_top", "rail", "chimney", "porch_floor", "doors", "plinths", "ivy", "ivy_mat", "tree_boost", "remap"],
    "creatures": ["voxel", "half_w", "height", "colors", "morphs", "behaviour", "in_water", "ops"],
    "collectibles": ["display", "voxel", "spin", "colors", "ops"],
    "biomes": ["priority", "rules", "surface", "subsurface", "rock", "fringe", "map_color", "plants", "features", "creatures", "collectibles", "sound"],
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
    i = src.find("ENGINE_MATERIALS")
    if i < 0:
        return []
    start, end = src.find("[", src.find("=", i)), src.find("]", src.find("=", i))
    if start < 0 or end < 0:
        return []
    return [s.strip().strip('"') for s in src[start + 1 : end].split(",") if s.strip()]


def plant_mats(plant, lib, hit):
    """Add every material a plant file names. Reading every string beats a list
    of keys to keep in step with the schema: bark, leaf, leaf_alt, flower,
    autumn, stalk and whatever comes next. The Rust side (`worldload::reach`)
    reads them the same way."""

    def walk(v):
        if isinstance(v, str):
            if v in lib["materials"]:
                hit.add(("materials", v))
        elif isinstance(v, list):
            for x in v:
                walk(x)
        elif isinstance(v, dict):
            for k, x in v.items():
                if k not in ("name", "notes", "introduced_by"):
                    walk(x)

    walk(plant)


def world_dirs():
    """Every world folder, as (label, path). `local/worlds` is kept out of the
    published game (local/README.md) but its worlds reach library entries just
    the same, so leaving it out would report those entries unused. They all
    answer to the one label "local", since CATALOGUE.md is published and an
    unpublished world's name is not."""
    out = []
    for root, published in [(os.path.join(ROOT, "worlds"), True), (os.path.join(ROOT, "local", "worlds"), False)]:
        for w in sorted(os.listdir(root)) if os.path.isdir(root) else []:
            if os.path.isdir(os.path.join(root, w)):
                out.append((w if published else "local", os.path.join(root, w)))
    return out


def world_uses(lib):
    """{(kind, name): [world, ...]} for every entry a world reaches."""
    uses = {}
    engine = engine_materials()
    for wname, wpath in world_dirs():
        try:
            with open(os.path.join(wpath, "world.json"), encoding="utf-8") as fh:
                wd = json.load(fh)
            with open(os.path.join(wpath, "chronicle.json"), encoding="utf-8") as fh:
                ch = json.load(fh)
        except (OSError, ValueError):
            continue
        hit = {("materials", m) for m in engine if m in lib["materials"]}
        # Particle effects are the engine's, thrown by events every world has.
        hit |= {("fx", n) for n in lib["fx"]}
        biomes = wd.get("biomes") or list(lib["biomes"])
        for b in biomes + [wd.get("default_biome"), wd.get("seabed_biome")]:
            bd = lib["biomes"].get(b)
            if not bd:
                continue
            hit.add(("biomes", b))
            for m in [bd.get("surface"), bd.get("subsurface"), bd.get("rock")]:
                if m:
                    hit.add(("materials", m))
            for p in bd.get("plants", []):
                hit.add(("plants", p["plant"]))
            for p in bd.get("features", []):
                hit.add(("structures", p["structure"]))
            for p in bd.get("creatures", []):
                hit.add(("creatures", p["creature"]))
            for p in bd.get("collectibles", []):
                hit.add(("collectibles", p["item"]))
        for m, _ in wd.get("rock", []) + wd.get("strata", []):
            hit.add(("materials", m))
        for gun in (wd.get("shooter") or {}).get("weapons", []):
            hit.add(("weapons", gun))

        def scan(v):
            if isinstance(v, dict):
                for k, x in v.items():
                    if k in ("structure",) and isinstance(x, str):
                        hit.add(("structures", x))
                    elif k == "structures" and isinstance(x, list):
                        for s in x:
                            hit.add(("structures", s))
                    elif k == "buildings" and isinstance(x, list):
                        for s in x:
                            hit.add(("structures", s))
                    elif k in ("mat", "bed", "water", "old_bed", "silt", "floor", "char", "cap", "shoulder", "tell", "path", "rock", "mantle", "lining", "wall", "bank", "apron") and isinstance(x, str):
                        hit.add(("materials", x))
                    elif k in ("kit", "street", "lane", "edge", "span") and isinstance(x, str) and x in lib["kits"]:
                        hit.add(("kits", x))
                    elif k == "kit" and isinstance(x, str) and x in lib["cities"]:
                        hit.add(("cities", x))
                    elif k == "style" and isinstance(x, str) and x in lib["signs"]:
                        hit.add(("signs", x))
                    elif k == "item" and isinstance(x, str):
                        hit.add(("collectibles", x))
                        hit.add(("collectibles", x + "_mother"))
                    elif k == "slots" and isinstance(x, dict):
                        for m in x.values():
                            hit.add(("materials", m))
                    else:
                        scan(x)
            elif isinstance(v, list):
                for x in v:
                    scan(x)

        scan(ch.get("events", []))
        # Structures pull in their slot materials and the materials their ops
        # name, plants their bark and leaves (a parent's too, by `like`).
        def op_mats(ops):
            for op in ops:
                m = op.get("mat")
                if isinstance(m, str) and m in lib["materials"]:
                    hit.add(("materials", m))
                op_mats(op.get("ops", []))

        for kind, name in list(hit):
            # A sign style brings in the post it stands on.
            if kind == "signs" and name in lib["signs"]:
                post = lib["signs"][name].get("post")
                if post:
                    hit.add(("structures", post))
        # A kit brings in every material it names and the kit it spans with.
        for _ in range(3):
            for kind, name in list(hit):
                if kind != "kits" or name not in lib["kits"]:
                    continue
                kd = lib["kits"][name]
                if kd.get("span"):
                    hit.add(("kits", kd["span"]))
                for key in ("mat", "shoulder"):
                    if isinstance(kd.get(key), str):
                        hit.add(("materials", kd[key]))
                for grp in ("rails", "posts"):
                    for r in kd.get(grp, []):
                        hit.add(("materials", r["mat"]))
                for key in ("deck", "walk", "piers"):
                    d = kd.get(key) or {}
                    for m in (d.get("mat"), d.get("under"), d.get("kerb")):
                        if m:
                            hit.add(("materials", m))
        for kind, name in list(hit):
            if kind == "structures" and name in lib["structures"]:
                for m in lib["structures"][name].get("slots", {}).values():
                    hit.add(("materials", m))
                op_mats(lib["structures"][name].get("ops", []))
            # A plant brings in its own materials and its parents', so walk the
            # `like` chain and read every link, not just the root: a child that
            # overrides `leaf` (blossom_vine) names a material no parent does.
            chain = []
            n = name
            while kind == "plants" and n in lib["plants"]:
                chain.append(n)
                n = lib["plants"][n].get("like")
                if n is None or n in chain:
                    break
                hit.add(("plants", n))
            for link in chain:
                plant_mats(lib["plants"][link], lib, hit)
        # A rare morph comes with its parent, as worldload::used_entries does
        # (a morph of a morph too), so the catalogue does not call it unused.
        for _ in range(3):
            for name, c in lib["creatures"].items():
                parent = c.get("like")
                if c.get("rarity", 0) > 0 and parent and ("creatures", parent) in hit:
                    hit.add(("creatures", name))
        for key in hit:
            uses.setdefault(key, []).append(wname)
    return uses


def catalogue(lib):
    uses = world_uses(lib)
    out = [
        "# Library catalogue",
        "",
        "Generated by `python tools/libfmt.py --catalogue`; do not edit by hand.",
        "Every entry, who introduced it, and the worlds that reach it (through their",
        "biome list, strata, chronicle events and structure slots). An entry no world",
        "uses is listed as unused; \"local\" is a world kept out of the published",
        "game (local/README.md).",
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
            u = ", ".join(sorted(set(uses.get((kind, d["name"]), [])))) or "unused"
            note = (d.get("notes") or "").replace("|", "/").replace("\n", " ")
            # The form or the verb, in front of the notes -- unless the notes
            # already open with it, which many of them do.
            lead = d.get("form", "") if kind == "plants" else d.get("behaviour", {}).get("movement", "") if kind == "creatures" else ""
            if lead and not note.startswith(lead + ":"):
                note = (lead + ": " + note).strip(": ")
            cells = [d["name"], d.get("introduced_by", "?"), u, note]
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
