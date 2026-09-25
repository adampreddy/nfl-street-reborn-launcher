"""Exercise the real fork bridge using two tiny profiles and one shared game image."""
from pathlib import Path
import subprocess,json,time,sys
root=Path('target/netplay-bridge-review').resolve();root.mkdir(exist_ok=True)
runtime=Path(sys.argv[1]).resolve();game=Path(sys.argv[2]).resolve()
assert runtime.is_file() and game.is_file()
assert b'--reborn-netplay-session' in runtime.read_bytes()
processes={};report={};seq=0

def heartbeat():
 for path in root.glob('*/session.json.alive'):path.write_text('alive')
def status(mode):
 try:return json.loads((root/mode/'session.json.status').read_text(encoding='utf-8'))
 except (OSError,json.JSONDecodeError):return {}
def wait_for(test,timeout=20):
 until=time.time()+timeout
 while time.time()<until:
  heartbeat()
  if test():return
  time.sleep(.25)
 raise AssertionError({mode:status(mode) for mode in processes})
def command(mode,action,**data):
 global seq
 seq+=1
 file=root/mode/'session.json.command';temp=file.with_suffix('.tmp');temp.write_text(json.dumps({'id':seq,'action':action,**data}),encoding='utf-8');temp.replace(file)
def launch(mode,connection='direct',address='127.0.0.1'):
 folder=root/mode;user=folder/'User';(user/'Config').mkdir(parents=True,exist_ok=True)
 card=folder/'personal-save.raw';card.write_bytes(b'Existing personal save must never be accessed or modified by this session.')
 (user/'Config'/'Dolphin.ini').write_text(f'[Core]\nSlotA = 1\nSlotB = 8\nMemcardAPath = {card}\nGFXBackend = D3D\nRAMOverrideEnable = True\nMEM1Size = 67108864\nSyncGPU = True\n[DSP]\nVolume = 0\n',encoding='utf-8')
 (user/'Config'/'GFX.ini').write_text('[Settings]\nInternalResolution = 3\nMSAA = 1\n[Hardware]\nVSync = True\n',encoding='utf-8')
 plan=folder/'session.json';plan.write_text(json.dumps({'game':str(game),'mode':'host' if mode=='host' else 'join'}),encoding='utf-8');Path(str(plan)+'.alive').write_text('alive')
 for suffix in ['.command','.status']:Path(str(plan)+suffix).unlink(missing_ok=True)
 args=[str(runtime),'-u',str(user),'--reborn-netplay-session',str(plan)]
 for value in ['Core.SlotA=255','Core.SlotB=255','NetPlay.SyncSaves=True','NetPlay.WriteSaveData=False','NetPlay.SyncCodes=True','NetPlay.StrictSettingsSync=True','NetPlay.UseIndex=False','Display.RenderToMain=False','Display.Fullscreen=False','Analytics.PermissionAsked=True','Interface.SkipNKitWarning=True',f'NetPlay.TraversalChoice={connection}','NetPlay.HostPort=26261','NetPlay.ConnectPort=26261',f'NetPlay.Address={address}','NetPlay.HostCode='+address,'NetPlay.Nickname=Test '+mode,'Interface.ConfirmStop=False']:
  args+=['-C','Dolphin.'+value]
 args+=['-C','Graphics.Settings.WaitForShadersBeforeStarting=False']
 processes[mode]=subprocess.Popen(args,cwd=runtime.parent,creationflags=0x08000000)
 (folder/'pid.txt').write_text(str(processes[mode].pid))
try:
 launch('host');wait_for(lambda:status('host').get('phase')=='lobby')
 launch('join');wait_for(lambda:len(status('host').get('players',[]))==2 and status('host').get('can_start'))
 report['connected']={m:status(m) for m in processes}
 command('host','ports',ports=[1,2,1,2]);wait_for(lambda:all(status(m).get('ports')==[1,2,1,2] for m in processes))
 report['four_slots_assigned']=True
 command('host','ports',ports=[1,2,0,0]);wait_for(lambda:all(status(m).get('ports')==[1,2,0,0] for m in processes))
 command('host','start');wait_for(lambda:all(status(m).get('phase')=='running' for m in processes),35)
 until=time.time()+12
 while time.time()<until:heartbeat();time.sleep(.5)
 for mode in processes:assert status(mode).get('memory_cards_off') is True,(mode,status(mode))
 report['running']={m:status(m) for m in processes}
 command('join','leave');wait_for(lambda:processes['join'].poll() is not None,15)
 command('host','leave');wait_for(lambda:processes['host'].poll() is not None,15)
 for mode in processes:
  ini=(root/mode/'User/Config/Dolphin.ini').read_text(encoding='utf-8')
  assert 'SlotA = 1' in ini and 'SlotB = 8' in ini
  assert (root/mode/'personal-save.raw').read_bytes()==b'Existing personal save must never be accessed or modified by this session.'
 report['clean_exit']=True;report['personal_cards_preserved']=True
 # Invalid direct connection errors must appear in the bridge, with no modal handoff.
 launch('join',address='127.0.0.1');wait_for(lambda:status('join').get('phase')=='error',30)
 report['invalid_join']=status('join');command('join','leave');wait_for(lambda:processes['join'].poll() is not None,15)
 (root/'result.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
 print(json.dumps(report,indent=2))
finally:
 for mode,proc in processes.items():
  if proc.poll() is None:
   try:command(mode,'leave');proc.wait(timeout=4)
   except:proc.kill();proc.wait()
