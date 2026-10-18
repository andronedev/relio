#!/usr/bin/env python3
"""Writes update.json, the manifest e-readers read before updating:
scripts/release-manifest.py VERSION PACKAGE SITE_URL OWNER/REPO > update.json"""

import hashlib
import json
import sys
from pathlib import Path

version, package, site, repository = sys.argv[1:5]
data = Path(package).read_bytes()
manifest = {
    "version": version,
    "size": len(data),
    "sha256": hashlib.sha256(data).hexdigest(),
    "urls": [
        f"{site.rstrip('/')}/KoboRoot.tgz",
        f"https://github.com/{repository}/releases/download/v{version}/KoboRoot.tgz",
    ],
}
print(json.dumps(manifest, indent=2))
