# Apply a pack with the proven Python chain onto COPIES of the given files, so
# the Rust `soundtrack::apply::build_files` output can be compared byte for
# byte (oracle 2 in docs/superpowers/specs/2026-09-08-music-editor-design.md).
#
#   py -3 Tools/oracle-pack-import.py <pack> <pathfind.dat> <template.dat> <out_dir>
#
# NSR_CLEAN names the nflstreet2_clean tree whose tools/ holds music_pack.py.
import os
import shutil
import sys

CLEAN = os.environ.get('NSR_CLEAN', r'C:\Users\Adam\nflstreet2_clean')
sys.path.insert(0, os.path.join(CLEAN, 'tools'))
import music_pack as P  # noqa: E402

pack, pf, tpl, out = sys.argv[1:5]
os.makedirs(out, exist_ok=True)
pf2 = os.path.join(out, 'pathfind.dat')
tpl2 = os.path.join(out, 'template.dat')
shutil.copy2(pf, pf2)
shutil.copy2(tpl, tpl2)
P.import_pack(pack, log=print, do_build=False, pathfind=pf2, template=tpl2)
print('wrote', pf2, tpl2)
