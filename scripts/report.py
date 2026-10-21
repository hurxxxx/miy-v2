#!/usr/bin/env python3
"""Collect actual evidence; missing checks are errors."""
import datetime,json,platform,subprocess
from pathlib import Path
root=Path(__file__).resolve().parent.parent
out=root/'.local/evidence'
reports={name:json.loads((out/(name+'.json')).read_text()) for name in ['runtime','database','codex','mcp']}
images={}
for image in ['postgres:18.3','quay.io/keycloak/keycloak:26.8.1','caddy:2.11.2']:
 images[image]=json.loads(subprocess.check_output(['docker','image','inspect',image,'--format','{{json .RepoDigests}}'],text=True))
summary={'timestamp':datetime.datetime.now(datetime.timezone.utc).isoformat(),'machine':platform.machine(),'os':platform.system(),'images':images,'checks':reports,'scope':'V0-V1; UI fixtures and DB preparation, no OCI app deployment'}
(out/'summary.json').write_text(json.dumps(summary,ensure_ascii=False,indent=2))
print('V0-V1 evidence collected in .local/evidence/summary.json')
