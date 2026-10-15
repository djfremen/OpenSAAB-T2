# SPDX-License-Identifier: MPL-2.0
"""Preserve the deployed collector/API; add only the private support inbox."""
import os
import server
from support_reports_api import make_router
app = server.app
app.include_router(make_router(server._s3_client, lambda: os.environ.get('OPENSAAB_SUPPORT_BUCKET', 'opensaab-support-reports'),
                               lambda: os.environ.get('OPENSAAB_ADMIN_TOKEN', '')))
