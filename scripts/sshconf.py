"""SSH settings for reaching the game server, shared by the deploy scripts.

Defaults fit the production VM; override on another machine with env vars:
    PULSE_SERVER   user@host                 (default avpetrov89@34.14.29.132)
    PULSE_SSH      ssh executable            (default: the portable one on the
                                              original PC if present, else ssh)
    PULSE_SSH_KEY  private key               (default ~/.ssh/id_ed25519)
The server's host key must already be in ~/.ssh/known_hosts (connect once
with plain `ssh` and accept it); scripts refuse unknown hosts.
"""
import os
from pathlib import Path

SERVER = os.environ.get('PULSE_SERVER', 'avpetrov89@34.14.29.132')
HOME = Path(os.environ.get('USERPROFILE') or Path.home())


def ssh_command():
    portable = Path('C:/0_WORK_INVIRONMENT/far3-portable/tools/ssh/ssh.exe')
    ssh = os.environ.get('PULSE_SSH') or (str(portable) if portable.exists() else 'ssh')
    key = os.environ.get('PULSE_SSH_KEY') or str(HOME / '.ssh' / 'id_ed25519')
    return [ssh, '-i', key, '-o', f'UserKnownHostsFile={(HOME / ".ssh" / "known_hosts").as_posix()}',
            '-o', 'BatchMode=yes', '-o', 'StrictHostKeyChecking=yes', SERVER]
