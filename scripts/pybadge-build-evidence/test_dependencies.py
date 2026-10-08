from pathlib import Path
import json
import tempfile
import unittest

from dependencies import capture_discovery, origin, parse_dependencies


class Admission(unittest.TestCase):
    def test_discovery_preserves_generated_metadata_not_objects(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            build, out = root / "build", root / "evidence"
            build.mkdir()
            out.mkdir()
            raw = b"# generated dependency recipe\na.o: /public/header.h\n"
            (build / "depend.make").write_bytes(raw)
            (build / "a.o").write_bytes(b"not captured")
            capture_discovery(build, root, out)
            self.assertEqual((out / "dependency-discovery/build/depend.make").read_bytes(), raw)
            self.assertFalse((out / "dependency-discovery/build/a.o").exists())
            report = json.loads((out / "dependency-discovery.json").read_text())
            self.assertEqual(len(report["inventory"]), 2)
            self.assertEqual(len(report["captured"]), 1)

    def test_original_gcc_continuations_and_escaped_spaces(self):
        target, deps = parse_dependencies("CMakeFiles/app.dir/screen.cpp.o: /public/screen.cpp \\\n /public/a\\ b.h /public/screen.cpp\n")
        self.assertEqual(target, "CMakeFiles/app.dir/screen.cpp.o")
        self.assertEqual(deps, ["/public/a b.h", "/public/screen.cpp"])

    def test_ambiguous_empty_and_variable_rules_rejected(self):
        for text in ("", "a.o:", "a.o b.o: a.h", "a.o: $(HEADER)", "a.o: a.h\nb.h:\n"):
            with self.assertRaises(ValueError):
                parse_dependencies(text)

    def test_build_and_toolchain_origins_are_distinct(self):
        self.assertEqual(origin(Path("/public/source/header.h"), Path("/public/source")),
                         ("build-source", "header.h"))
        self.assertEqual(origin(Path("/usr/include/header.h"), Path("/public/source")),
                         ("toolchain", "include/header.h"))
        with self.assertRaises(ValueError):
            origin(Path("/unreviewed/header.h"), Path("/public/source"))


if __name__ == "__main__":
    unittest.main()
