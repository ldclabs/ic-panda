#!/usr/bin/env python3
"""Build/sign the native module; never launches, registers, or enables it.
Use --provision only when authorized to update the Apple Developer account.
Distribution builds require a matching FSKit Module provisioning profile.
"""
import argparse,pathlib,shutil,subprocess
p=argparse.ArgumentParser()
p.add_argument('--output',required=True)
p.add_argument('--team',required=True)
p.add_argument('--identity',default='Apple Development')
p.add_argument('--profile',help='Explicit provisioning profile name for manual distribution signing')
p.add_argument('--configuration',choices=['Debug','Release'],default='Debug')
p.add_argument('--provision',action='store_true')
a=p.parse_args()
src=pathlib.Path(__file__).parent.resolve();out=pathlib.Path(a.output).resolve();project=out/'project';project.mkdir(parents=True,exist_ok=True)
for source in src.iterdir():
 if source.suffix in ['.swift','.plist','.entitlements','.yml']:shutil.copyfile(source,project/source.name)
subprocess.run(['xcodegen','generate','--spec',str(project/'project.yml'),'--project',str(project)],check=True)
command=['xcodebuild','-project',str(project/'DmsgSecretsProbe.xcodeproj'),'-scheme','SecretsFS','-configuration',a.configuration,'-derivedDataPath',str(out/'derived'),'CODE_SIGN_STYLE='+('Manual' if a.profile else 'Automatic'),'CODE_SIGN_IDENTITY='+a.identity,'DEVELOPMENT_TEAM='+a.team,'build']
if a.profile:command.append('PROVISIONING_PROFILE_SPECIFIER='+a.profile)
if a.provision:command[1:1]=['-allowProvisioningUpdates']
subprocess.run(command,check=True)
extension=out/f'derived/Build/Products/{a.configuration}/SecretsFS.appex'
if not (extension/'Contents/embedded.provisionprofile').is_file():raise SystemExit('Missing FSKit Module provisioning profile')
subprocess.run(['codesign','--verify','--strict',str(extension)],check=True)
print(extension)
