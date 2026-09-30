"""Upload a prepared release through existing SSH credentials, then install it.
Explicit invocation performs deployment; build/tests/package happen beforehand.
"""
from pathlib import Path
import hashlib
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from sshconf import ssh_command  # noqa: E402  (server, key, ssh binary; env-overridable)

root=Path(__file__).resolve().parent.parent
args=ssh_command()
archive=root/'pulse-server.zip'
digest=hashlib.sha256(archive.read_bytes()).hexdigest()
remote=f'/home/avpetrov89/pulse-arena/incoming/{digest}.zip'
with archive.open('rb') as payload:
    subprocess.run(args+['mkdir -p /home/avpetrov89/pulse-arena/incoming && cat > '+remote],stdin=payload,check=True)
with (root/'scripts/install-server.py').open('rb') as script:
    subprocess.run(args+[f'python3 - {remote} {digest}'],stdin=script,check=True)
