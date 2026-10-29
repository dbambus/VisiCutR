#!/usr/bin/env bash
# Writes a PLF case file with Java VisiCut (MakePlf.java), e.g.
#
#   bash scripts/java-parity/make-plf.sh tests/java_parity/plf_two_parts/project.plf \
#     "a.svg|20|30|cut: all" "b.svg|120|40|1.5|30|cut: Stroke Color=#ff0000; mark: *"
#
# File names are relative to the folder of the PLF. Uses the build of
# generate.sh (run it once first). See docs/Java-Vergleich.md.
set -euo pipefail

work="${TMPDIR:-/tmp}/visicut-java-parity"
# The Java tree of generate.sh (tag java-reference, or JAVA_TREE).
jar="${JAVA_TREE:-$work/java}/target/visicut-1.9-SNAPSHOT-full.jar"
out="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
shift
# VisiCut keeps its temporary files in ~/.visicut.
mkdir -p "$work/home/.visicut"
# MakePlf reads the graphics relative to the folder of the PLF.
cd "$(dirname "$out")"
java --add-opens=java.base/java.util=ALL-UNNAMED --add-opens=java.base/java.lang=ALL-UNNAMED \
  --add-opens=java.desktop/java.awt.geom=ALL-UNNAMED --add-opens=java.desktop/java.awt=ALL-UNNAMED \
  -Djava.awt.headless=true -Duser.home="$work/home" \
  -cp "$jar:$work/classes" MakePlf "$out" "$@"
