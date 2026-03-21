"""Entry point for the clayers CLI when installed via pip/uv."""

import sys


def main():
    # Replace argv[0] with "clayers" so clap sees the right program name
    # and doesn't interpret the Python script path as a subcommand.
    sys.argv[0] = "clayers"

    from clayers._clayers import cli_main

    cli_main()


if __name__ == "__main__":
    main()
