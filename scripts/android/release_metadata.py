"""Release identity packaged once, never inferred from installation or launch time."""
from datetime import date
import os
import re


def preview_metadata(private_preview, release_date=None, version_name=None, release_channel='developer_preview'):
    value = release_date or os.environ.get('OPENSAAB_RELEASE_DATE')
    if not value:
        if not private_preview:
            raise ValueError('Published builds require an explicit OPENSAAB_RELEASE_DATE or --release-date')
        value = date.today().isoformat()
    if not re.fullmatch(r'\d{4}-\d{2}-\d{2}', value):
        raise ValueError('Release date must be YYYY-MM-DD')
    date.fromisoformat(value)
    if release_channel not in ('developer_preview','qualified'):
        raise ValueError('Unknown release channel')
    metadata = {'release_date': value, 'release_channel': release_channel}
    if version_name is not None:
        metadata['security_access_mode'] = 'no_operator_password'
    return metadata
