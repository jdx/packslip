"""Exercise the action's actual create script without contacting signing services."""

import itertools
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest


ACTION = (Path(__file__).resolve().parents[1] / "action.yml").read_text()
CREATE = ACTION.split("      id: create\n", 1)[1].split("      run: |\n", 1)
SCRIPT = "\n".join(
    line[8:] for line in itertools.takewhile(
        lambda line: not line or line.startswith("        "), CREATE[1].splitlines()
    )
)


class ActionCommitTest(unittest.TestCase):
    def run_create(self, commit):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cli = root / "packslip"
            cli.write_text(
                "#!/usr/bin/env python3\n"
                "import json, pathlib, sys\n"
                "pathlib.Path('args.json').write_text(json.dumps(sys.argv[1:]))\n"
                "pathlib.Path('out').mkdir(exist_ok=True)\n"
                "pathlib.Path('out/packslip.sigstore.json').touch()\n"
            )
            cli.chmod(0o755)
            (root / "packslip-files").write_text("dist/tool-linux-x64.tar.xz\n")
            env = dict(os.environ)
            # Populate the action input environment from its real wiring, so an
            # absent or miswired commit input cannot silently pass the test.
            inputs = {"commit": commit, "tag": "v1.2.3", "out": "out", "attest": "false"}
            for name, key in re.findall(
                r"^        (IN_\w+): \$\{\{ inputs\.([\w-]+) \}\}$",
                CREATE[0],
                re.MULTILINE,
            ):
                env[name] = inputs.get(key, "")
            env.update(
                PATH=str(root) + os.pathsep + env["PATH"],
                RUNNER_TEMP=str(root),
                GITHUB_OUTPUT=str(root / "outputs"),
                REPO="owner/tool",
                REF_NAME="main",
                REF_TYPE="branch",
                SHA="a" * 40,
                SERVER="https://github.com",
                API="https://api.github.com",
            )
            subprocess.run(["bash", "-eu", "-c", SCRIPT], cwd=root, env=env, check=True)
            args = json.loads((root / "args.json").read_text())
            self.assertEqual(args[args.index("--tag") + 1], "v1.2.3")
            self.assertEqual(args[args.index("--version") + 1], "1.2.3")
            return args[args.index("--commit") + 1]

    def test_default_preserves_workflow_commit(self):
        self.assertEqual(self.run_create(""), "a" * 40)

    def test_manual_release_can_override_workflow_commit(self):
        self.assertEqual(self.run_create("b" * 40), "b" * 40)

    def test_override_is_one_literal_argument_not_shell_code(self):
        # The real CLI validates the commit. The action must pass it literally.
        commit = "$(exit 77) two words"
        self.assertEqual(self.run_create(commit), commit)


if __name__ == "__main__":
    unittest.main()
