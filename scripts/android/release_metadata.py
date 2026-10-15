"""Release identity packaged once, never inferred from installation or launch time."""
from datetime import date
import os
import re


def preview_metadata(private_preview, release_date=None):
    value = release_date or os.environ.get('OPENSAAB_RELEASE_DATE')
    if not value:
        if not private_preview:
            raise ValueError('Published builds require an explicit OPENSAAB_RELEASE_DATE or --release-date')
        value = date.today().isoformat()
    if not re.fullmatch(r'\d{4}-\d{2}-\d{2}', value):
        raise ValueError('Release date must be YYYY-MM-DD')
    date.fromisoformat(value)
    return {'release_date': value, 'release_channel': 'developer_preview'}
