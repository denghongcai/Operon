#!/usr/bin/env python3
"""Offline negative acceptance: native SDK/embedded gates cannot be bypassed."""
import os
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="operon-release-gate-contract-") as directory:
    mock = Path(directory) / "gh"
    mock.write_text("""#!/usr/bin/env python3
import os, sys
args = sys.argv[1:]
if args[:2] != ['run', 'list'] or '--workflow' not in args:
    sys.exit(3)
workflow = args[args.index('--workflow') + 1]
if workflow != os.environ['OPERON_TEST_MISSING_WORKFLOW']:
    print('123')
""")
    mock.chmod(0o700)
    for workflow in ("Native SDK Packages", "Embedded Connectivity"):
        environment = dict(os.environ, PATH=directory + os.pathsep + os.environ["PATH"],
                           OPERON_TEST_MISSING_WORKFLOW=workflow)
        result = subprocess.run(["bash", "scripts/release-gate-orchestrate.sh", "pretag",
                                 "v0.16.15", "a" * 40, "test/operon"], cwd=root,
                                env=environment, capture_output=True, text=True, timeout=30)
        assert result.returncode == 1, result.stdout + result.stderr
        assert f"missing successful workflow '{workflow}'" in result.stderr, result.stderr
        print("PASS pretag refuses missing " + workflow)
