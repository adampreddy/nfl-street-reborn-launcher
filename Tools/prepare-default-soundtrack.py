"""Bake and verify a default soundtrack update locally. Never uploads anything.

Only pathfind.dat and the 21 song banners in template.dat may change. The
source ISO is read-only; temporary rebuilt images are removed after verification.
The output is a patch chain and a provenance report, with no game image or saves.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
import zipfile


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def run(*args):
    subprocess.run([str(a) for a in args], check=True)


def files(path):
    with path.open("rb") as stream:
        header = stream.read(0x440)
        assert header[:6] == b"GN7E69", "Wrong game ID"
        offset, size = struct.unpack_from(">II", header, 0x424)
        stream.seek(offset)
        fst = stream.read(size)
        count = struct.unpack_from(">I", fst, 8)[0]
        names = fst[count * 12:]
        result, stack = {}, [(count, "")]
        for i in range(1, count):
            while i >= stack[-1][0]:
                stack.pop()
            tag, pos, size = struct.unpack_from(">III", fst, i * 12)
            name_offset = tag & 0xffffff
            name = names[name_offset:names.index(0, name_offset)].decode("ascii")
            name = stack[-1][1] + name
            if tag >> 24:
                stack.append((size, name + "/"))
            else:
                stream.seek(pos)
                data = stream.read(size)
                result[name] = (hashlib.sha256(data).hexdigest(), data if name.endswith("template.dat") else None)
        # The executable is outside the FST. Verify it separately, too.
        dol_offset = struct.unpack_from(">I", header, 0x420)[0]
        stream.seek(dol_offset)
        dol_header = stream.read(0x100)
        ends = [struct.unpack_from(">I", dol_header, i * 4)[0] + struct.unpack_from(">I", dol_header, 0x90 + i * 4)[0] for i in range(18)]
        stream.seek(dol_offset)
        result["@main.dol"] = (hashlib.sha256(stream.read(max(ends))).hexdigest(), None)
        return result


def without_song_banners(data):
    data = bytearray(data)
    start = data.find(b"DINS")
    while start >= 0:
        if all(data[start + i * 16:start + i * 16 + 4] == tag for i, tag in enumerate([b"DINS", b"ITNS", b"LANS", b"RANS", b"XINS"])):
            break
        start = data.find(b"DINS", start + 4)
    assert start >= 0, "Song table missing"
    for slot in range(21):
        record = start + 72 + slot * 156
        # Keep the song ID and flags. Only title, album and artist may change.
        data[record + 4:record + 154] = bytes(150)
    return data


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ["source", "pack", "chain", "ctl", "xdelta", "output"]:
        parser.add_argument("--" + name, required=True, type=Path)
    parser.add_argument("--label", default="Reborn default soundtrack")
    args = parser.parse_args()
    source, out = args.source.resolve(), args.output.resolve()
    source_sha = sha(source)
    chain = json.loads(args.chain.read_text(encoding="utf-8-sig"))
    assert chain["steps"][-1]["to_sha256"] == source_sha, "Source is not the chain head"
    with zipfile.ZipFile(args.pack) as archive:
        expected = json.loads(archive.read("pack.json"))
        assert len(expected["songs"]) == 21, "The default must supply all 21 songs"
    out.mkdir(parents=True, exist_ok=False)
    work = Path(tempfile.mkdtemp(prefix=".verify-", dir=out)).resolve()
    try:
        assert work.is_relative_to(out)
        root = work / "data"
        pristine = root / "library/imported" / source_sha / "NSR.iso"
        booted = root / "library/offline/NSR.iso"
        for link in [pristine, booted]:
            link.parent.mkdir(parents=True, exist_ok=True)
            os.link(source, link)
        run(args.ctl, "music", "import", args.pack, "--data-root", root)
        run(args.ctl, "music", "enable", "--data-root", root)
        rebuilt_sha = sha(booted)
        assert rebuilt_sha != source_sha
        before, after = files(source), files(booted)
        assert before.keys() == after.keys(), "Disc file list changed"
        changed = [name for name in before if before[name][0] != after[name][0]]
        assert sorted(Path(name).name for name in changed) == ["pathfind.dat", "template.dat"], changed
        template = next(name for name in changed if name.endswith("template.dat"))
        assert without_song_banners(before[template][1]) == without_song_banners(after[template][1]), "Non-music roster data changed"
        export = work / "readback.nsrmusic"
        run(args.ctl, "music", "pack-from-image", "--from", booted, "--out", export)
        with zipfile.ZipFile(args.pack) as wanted, zipfile.ZipFile(export) as actual:
            found = json.loads(actual.read("pack.json"))
            for slot, song in expected["songs"].items():
                got = found["songs"][slot]
                for field in ["title", "artist", "album"]:
                    assert got[field] == song[field], (slot, field, got[field], song[field])
                assert actual.read(got["blob"]) == wanted.read(song["blob"]), f"Audio differs in slot {slot}"
        patch_name = f"patches/{source_sha[:12]}-{rebuilt_sha[:12]}.xdelta"
        patch = out / patch_name
        patch.parent.mkdir()
        window = ((source.stat().st_size + (1 << 20) - 1) // (1 << 20)) * (1 << 20)
        run(args.xdelta, "-e", "-B", window, "-s", source, booted, patch)
        decoded = work / "decoded.iso"
        run(args.xdelta, "-d", "-s", source, patch, decoded)
        assert sha(decoded) == rebuilt_sha, "Patch did not reproduce the target"
        assert sha(source) == source_sha, "Source changed during the build"
        for step in chain["steps"]:
            src = (args.chain.parent / step["patch"]).resolve()
            dst = (out / step["patch"]).resolve()
            assert dst.is_relative_to(out)
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(src, dst)
        chain["steps"].append(dict(from_sha256=source_sha, to_sha256=rebuilt_sha,
                                  to_bytes=booted.stat().st_size, patch=patch_name, label=args.label))
        (out / "chain.json").write_text(json.dumps(chain, indent=2) + "\n", encoding="utf-8")
        report = dict(source=str(source), source_sha256=source_sha, output_sha256=rebuilt_sha,
                      pack=str(args.pack), pack_sha256=sha(args.pack), patch_sha256=sha(patch),
                      changed_files=changed, audio_slots_verified=21, other_disc_files_unchanged=True,
                      non_music_template_data_unchanged=True, patch_decode_verified=True,
                      songs={slot: {k: song[k] for k in ["title", "artist", "album"]} for slot, song in expected["songs"].items()},
                      published=False)
        (out / "verification.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(f"Verified local soundtrack chain: {out}", flush=True)
    finally:
        assert work.is_relative_to(out) and work.name.startswith(".verify-")
        shutil.rmtree(work)


if __name__ == "__main__":
    main()
