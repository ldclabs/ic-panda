#!/usr/bin/env python3
"""Build the synthetic FSKit gate using Xcode's real ExtensionKit signing flow."""
import argparse,pathlib,shutil,subprocess
p=argparse.ArgumentParser();p.add_argument('--output',required=True);p.add_argument('--team',required=True);p.add_argument('--provision',action='store_true',help='Allow Xcode to update the selected Apple Developer account');a=p.parse_args()
src=pathlib.Path(__file__).parent.resolve();out=pathlib.Path(a.output).resolve();project=out/'project';project.mkdir(parents=True,exist_ok=True)
for source in src.iterdir():
 if source.suffix in ['.swift','.plist','.entitlements','.yml']:shutil.copyfile(source,project/source.name)
subprocess.run(['xcodegen','generate','--spec',str(project/'project.yml'),'--project',str(project)],check=True)
command=['xcodebuild','-project',str(project/'DmsgSecretsProbe.xcodeproj'),'-scheme','ProbeHost','-configuration','Debug','-derivedDataPath',str(out/'derived'),'CODE_SIGN_STYLE=Automatic','CODE_SIGN_IDENTITY=Apple Development','DEVELOPMENT_TEAM='+a.team,'build']
if a.provision:command[1:1]=['-allowProvisioningUpdates','-allowProvisioningDeviceRegistration']
subprocess.run(command,check=True)
app=out/'derived/Build/Products/Debug/dMsg FSKit Probe.app';extension=app/'Contents/Extensions/SecretsProbe.appex'
if not (extension/'Contents/embedded.provisionprofile').is_file():raise SystemExit('Missing FSKit Module provisioning profile; probe is not runnable')
subprocess.run(['codesign','--verify','--deep','--strict',str(app)],check=True)
print(app)
