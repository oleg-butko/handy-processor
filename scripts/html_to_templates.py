#!/usr/bin/env python3
"""Extract three-level templates from HTML paragraphs, one input line at a time."""

import argparse
from html import unescape
from html.parser import HTMLParser
from pathlib import Path


class TemplateParser(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=False)
        self.parts = None
        self.separators = 0
        self.templates = []

    def handle_starttag(self, tag, attrs):
        if tag == "p":
            self.parts = [] if any(name == "data-line" for name, _ in attrs) else None
            self.separators = 0

    def handle_data(self, data):
        if self.parts is not None:
            self.parts.append(data)

    def handle_entityref(self, name):
        if self.parts is not None:
            self.separators += name == "gt"
            self.parts.append(unescape(f"&{name};"))

    def handle_charref(self, name):
        self.handle_data(unescape(f"&#{name};"))

    def handle_endtag(self, tag):
        if tag == "p" and self.parts is not None:
            levels = [part.strip() for part in "".join(self.parts).split(">")]
            if self.separators == 2 and len(levels) == 3 and all(levels):
                self.templates.append(" > ".join(levels))
            self.parts = None


def output_path(source: Path) -> Path:
    if "input" in source.name:
        return source.with_name(source.name.replace("input", "output"))
    return source.with_name(f"{source.stem}-output{source.suffix}")


def main():
    arguments = argparse.ArgumentParser(description=__doc__)
    arguments.add_argument(
        "input_file", nargs="?", default="html-raw-input.txt",
        help="Input file in UTF-8 (default: html-raw-input.txt)",
    )
    source = Path(arguments.parse_args().input_file)
    destination = output_path(source)
    templates = []
    try:
        with source.open(encoding="utf-8-sig") as file:
            for line in file:
                parser = TemplateParser()
                parser.feed(line)
                parser.close()
                templates.extend(parser.templates)
        destination.write_text(
            "".join(template + "\n" for template in templates),
            encoding="utf-8", newline="\n",
        )
    except (OSError, UnicodeError) as error:
        arguments.exit(1, f"Error: {error}\n")
    print(f"Saved {len(templates)} templates to {destination}")


if __name__ == "__main__":
    main()
