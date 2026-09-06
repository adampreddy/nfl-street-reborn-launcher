"""Build a file-normalized Reborn delta from the team's source and target images.

python Tools/create-game-patch.py --source ORIGINAL --target REBORN --dolphin-tool
  DolphinTool.exe --xdelta xdelta3.exe --output NEW_BUNDLE_DIRECTORY
The destination must not exist. Original images are only read.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

def sha(file):
    digest = hashlib.sha256()
    with file.open('rb') as stream:
        while chunk := stream.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['source', 'target', 'dolphin-tool', 'xdelta', 'output']:
        parser.add_argument('--' + name, required=True, type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    with tempfile.TemporaryDirectory(prefix='reborn-patch-') as temp:
        temp = Path(temp)
        disc = temp / 'disc'
        subprocess.run([str(args.dolphin_tool.resolve()), 'extract', '-i', str(args.source.resolve()), '-o', str(disc), '-q'], check=True)
        if (disc / 'sys/boot.bin').read_bytes()[:6] != b'GN7E69':
            raise ValueError('Source must be NFL Street 2 (USA), GameCube.')
        canonical = temp / 'source.bin'
        items = []
        with canonical.open('wb') as out:
            for file in sorted((disc / 'files').rglob('*')):
                if not file.is_file():
                    continue
                items.append(dict(path=file.relative_to(disc).as_posix(), bytes=file.stat().st_size, sha256=sha(file)))
                with file.open('rb') as inp:
                    shutil.copyfileobj(inp, out, 1024 * 1024)
        subprocess.run([str(args.xdelta.resolve()), '-e', '-B', '268435456', '-s', str(canonical), str(args.target.resolve()), str((args.output / 'reborn.xdelta').resolve())], check=True)
        manifest = dict(schema='nsr.game-patch.v1', disc_id='GN7E69', source_files=items, source_sha256=sha(canonical), output_sha256=sha(args.target), output_bytes=args.target.stat().st_size, patch='reborn.xdelta')
        (args.output / 'manifest.json').write_text(json.dumps(manifest, indent=2), encoding='utf-8')
        shutil.copy2(args.xdelta, args.output / 'xdelta3.exe')
        shutil.copy2(args.dolphin_tool, args.output / 'DolphinTool.exe')
    print('Bundle built. Run the production import_game example to verify it before packaging. Include tool licenses, source and required runtime DLLs.')

if __name__ == '__main__':
    main()
