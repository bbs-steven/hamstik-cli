#!/usr/bin/env python3
"""Run the Hamstik CLI's local quality gates in fail-fast order.

The checks mirror the repository's canonical gate list (AGENTS.md "Quality
gates") plus the advisory/license gate:

    cargo fmt --all --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo test --workspace
    cargo build --workspace --release
    cargo deny check advisories licenses sources
    git diff --check

Examples:
    ./scripts/do-prechecks.py
    ./scripts/do-prechecks.py --skip-deny      # when cargo-deny is unavailable
    ./scripts/do-prechecks.py --only fmt,clippy
"""

from __future__ import annotations

import argparse
import os
import re
import shlex
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Sequence

try:
    from rich import box
    from rich.console import Console
    from rich.markup import escape
    from rich.panel import Panel
    from rich.table import Table
    from rich.text import Text
except ModuleNotFoundError:
    print(
        "Rich is required for this precheck utility. Install it for the active "
        "interpreter with: python3 -m pip install rich",
        file=sys.stderr,
    )
    raise SystemExit(2)


REPO_ROOT = Path(__file__).resolve().parents[1]
ANSI_ESCAPE_PATTERN = re.compile(
    r"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07]*(?:\x07|\x1b\\))"
)
COMPILER_WARNING_PATTERN = re.compile(
    r"\bwarning(?:s)?:\s|\bwarning: unused\b|(?:^|\s)⚠(?:\s|$)",
    re.IGNORECASE,
)

console = Console(highlight=False)


@dataclass(frozen=True)
class Check:
    """One fail-fast quality gate."""

    name: str
    command: tuple[str, ...]
    detail: str
    reject_warnings: bool = False
    require_files: tuple[str, ...] = ()
    require_tools: tuple[str, ...] = ()


@dataclass(frozen=True)
class CheckResult:
    """The result of running one quality gate."""

    check: Check
    returncode: int
    elapsed_seconds: float
    warning_lines: tuple[str, ...] = ()

    @property
    def passed(self) -> bool:
        return self.returncode == 0 and not self.warning_lines


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Run the repository's quality gates in fail-fast order: format, "
            "lint, tests, then the production release build. Stop immediately "
            "when a check fails."
        )
    )
    parser.add_argument(
        "--skip-deny",
        action="store_true",
        help=(
            "Skip the cargo-deny advisory/license/source gate (useful when "
            "cargo-deny is not installed locally; CI still enforces it)."
        ),
    )
    parser.add_argument(
        "--skip-build",
        action="store_true",
        help="Skip the release build (useful for quick edit-verify loops).",
    )
    parser.add_argument(
        "--only",
        default=None,
        help=(
            "Comma-separated subset of checks to run "
            "(format, clippy, tests, release, advisories, whitespace)."
        ),
    )
    return parser.parse_args()


def checks_for(*, skip_deny: bool, skip_build: bool, only: str | None) -> list[Check]:
    """Return checks ordered from quick feedback to longest-running work."""
    checks = [
        Check(
            name="Format",
            command=("cargo", "fmt", "--all", "--check"),
            detail="Verify every file matches rustfmt's canonical formatting.",
        ),
        Check(
            name="Clippy",
            command=(
                "cargo",
                "clippy",
                "--workspace",
                "--all-targets",
                "--all-features",
                "--",
                "-D",
                "warnings",
            ),
            detail="Lint all targets with warnings denied (includes missing_docs and unwrap/expect lints).",
        ),
        Check(
            name="Tests",
            command=("cargo", "test", "--workspace"),
            detail="Run the complete unit, doc, and integration test suites.",
        ),
        Check(
            name="Release build",
            command=("cargo", "build", "--workspace", "--release"),
            detail="Build the optimized release binary and reject warning output.",
            reject_warnings=True,
        ),
        Check(
            name="Advisories and licenses",
            command=("cargo", "deny", "check", "advisories", "licenses", "sources"),
            detail=(
                "Enforce RUSTSEC advisory, dependency license, and registry "
                "source policy against the committed lockfile."
            ),
            require_files=("deny.toml",),
            require_tools=("cargo-deny",),
        ),
        Check(
            name="Whitespace",
            command=("git", "diff", "--check"),
            detail="Reject trailing whitespace and conflict markers in the diff.",
        ),
    ]

    if skip_build:
        checks = [check for check in checks if check.name != "Release build"]
    if skip_deny:
        checks = [check for check in checks if check.name != "Advisories and licenses"]

    if only is not None:
        selected = {name.strip().lower() for name in only.split(",") if name.strip()}
        known = {check.name.split()[0].lower(): check.name for check in checks}
        unknown = selected - set(known)
        if unknown:
            console.print(
                Panel(
                    "Unknown --only value(s): "
                    + ", ".join(sorted(unknown))
                    + f" (available: {', '.join(sorted(known))})",
                    title="[red]Cannot run prechecks[/red]",
                    border_style="red",
                )
            )
            raise SystemExit(2)
        checks = [check for check in checks if check.name.split()[0].lower() in selected]

    return checks


def validate_environment(checks: Sequence[Check]) -> None:
    """Fail before doing work when the checkout cannot run project commands."""
    problems: list[str] = []
    if shutil.which("cargo") is None:
        problems.append("cargo is not available on PATH")
    if not (REPO_ROOT / "Cargo.toml").is_file():
        problems.append(f"Cargo.toml was not found under {REPO_ROOT}")

    required_tools = {
        tool for check in checks for tool in check.require_tools
    }
    for tool in sorted(required_tools):
        if shutil.which(tool) is None:
            problems.append(f"{tool} is not available on PATH (required by an enabled check)")
    required_files = {
        file for check in checks for file in check.require_files
    }
    for file in sorted(required_files):
        if not (REPO_ROOT / file).is_file():
            problems.append(f"{file} was not found under {REPO_ROOT}")

    if problems:
        message = "\n".join(f"• {problem}" for problem in problems)
        console.print(
            Panel(
                message,
                title="[red]Cannot run prechecks[/red]",
                border_style="red",
            )
        )
        raise SystemExit(2)


def render_command(command: Sequence[str]) -> str:
    return shlex.join(str(part) for part in command)


def strip_terminal_codes(value: str) -> str:
    return ANSI_ESCAPE_PATTERN.sub("", value)


def print_process_line(line: str) -> None:
    """Render child output live while preserving any ANSI styling it contains."""
    value = line.rstrip("\r\n")
    if value:
        console.print(Text.from_ansi(value), soft_wrap=True)
    else:
        console.print()


def run_check(check: Check, *, position: int, total: int) -> CheckResult:
    console.print()
    console.rule(f"[bold cyan]{position}/{total} · {escape(check.name)}[/bold cyan]")
    console.print(check.detail)
    console.print(f"[dim]$ {escape(render_command(check.command))}[/dim]")

    started_at = time.monotonic()
    warning_lines: list[str] = []
    environment = os.environ.copy()
    process: subprocess.Popen[str] | None = None

    try:
        process = subprocess.Popen(
            list(check.command),
            cwd=REPO_ROOT,
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",
            bufsize=1,
        )
        assert process.stdout is not None
        for line in process.stdout:
            print_process_line(line)
            normalized = strip_terminal_codes(line).strip()
            if (
                check.reject_warnings
                and normalized
                and COMPILER_WARNING_PATTERN.search(normalized)
            ):
                warning_lines.append(normalized)
        returncode = process.wait()
    except KeyboardInterrupt:
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        console.print("\n[yellow]Prechecks interrupted.[/yellow]")
        raise SystemExit(130)

    elapsed = time.monotonic() - started_at
    return CheckResult(
        check=check,
        returncode=returncode,
        elapsed_seconds=elapsed,
        warning_lines=tuple(dict.fromkeys(warning_lines)),
    )


def print_failure(result: CheckResult, *, remaining: Sequence[Check]) -> None:
    reasons: list[str] = []
    if result.returncode != 0:
        reasons.append(f"Command exited with status {result.returncode}.")
    if result.warning_lines:
        reasons.append(
            f"The build emitted {len(result.warning_lines)} unique warning "
            f"marker{'s' if len(result.warning_lines) != 1 else ''}:"
        )
        reasons.extend(f"  • {escape(line)}" for line in result.warning_lines)
    if remaining:
        reasons.append(
            "Fail-fast mode did not run: " + ", ".join(check.name for check in remaining)
        )

    console.print()
    console.print(
        Panel(
            "\n".join(reasons),
            title=f"[bold red]Failed · {escape(result.check.name)}[/bold red]",
            border_style="red",
        )
    )


def print_success(results: Sequence[CheckResult]) -> None:
    table = Table(box=box.SIMPLE, show_header=True, header_style="bold")
    table.add_column("Check")
    table.add_column("Result", justify="center")
    table.add_column("Time", justify="right")
    for result in results:
        table.add_row(
            result.check.name,
            "[green]PASS[/green]",
            f"{result.elapsed_seconds:.1f}s",
        )

    total_seconds = sum(result.elapsed_seconds for result in results)
    console.print()
    console.print(table)
    console.print(
        Panel(
            f"[bold green]All {len(results)} prechecks passed[/bold green] "
            f"in {total_seconds:.1f}s.",
            border_style="green",
        )
    )


def main() -> int:
    options = parse_args()
    checks = checks_for(
        skip_deny=options.skip_deny,
        skip_build=options.skip_build,
        only=options.only,
    )
    validate_environment(checks)

    console.print(
        Panel(
            "Quick gates run first; execution stops on the first failure.",
            title="[bold]Hamstik CLI prechecks[/bold]",
            border_style="cyan",
        )
    )

    results: list[CheckResult] = []
    for index, check in enumerate(checks):
        result = run_check(check, position=index + 1, total=len(checks))
        results.append(result)
        if not result.passed:
            print_failure(result, remaining=checks[index + 1 :])
            return result.returncode if result.returncode else 1
        console.print(
            f"[bold green]PASS[/bold green] {escape(check.name)} "
            f"[dim]({result.elapsed_seconds:.1f}s)[/dim]"
        )

    print_success(results)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())