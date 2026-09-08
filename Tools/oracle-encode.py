# Run extend_slot_40big.py unchanged, but redirect its output ASF to a file we
# name, then cut the slot's 40-stream blob out of it.
#   py -3 Tools/oracle-encode.py <wav24k.wav> <slot> <base pathfind.dat> <out.asf-blob>
import os, sys
CLEAN = os.environ.get('NSR_CLEAN', r'C:\Users\Adam\nflstreet2_clean')
sys.path.insert(0, os.path.join(CLEAN, 'tools'))
import extend_song_slot as X
wav, slot, base, out = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4]
X.ASF = out + '.full.asf'
import extend_slot_40big as S
sys.argv = ['extend_slot_40big.py', wav, str(slot), base]
S.main()
import music_pack as P
d = open(X.ASF, 'rb').read()
first = P.slot_first(slot)
asf, pos = P._asf_pos(b'\x00' * P.ASF_OFF + d)
open(out, 'wb').write(bytes(asf[pos[first]:pos[first + 40]]))
print('blob', out, os.path.getsize(out))
