from pathlib import Path
import unittest

from dependencies import origin, parse_dependencies


class Admission(unittest.TestCase):
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
