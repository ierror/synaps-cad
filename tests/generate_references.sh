#!/usr/bin/env bash
# Generate OpenSCAD reference data (bounding box, vertex/facet counts) for comparison tests.
# Usage: bash tests/generate_references.sh
#
# Requires: openscad CLI, timeout (coreutils)

set -uo pipefail

OPENSCAD="${OPENSCAD:-/opt/homebrew/bin/openscad}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
EXAMPLES_DIR="$SCRIPT_DIR/openscad_examples"
REF_DIR="$SCRIPT_DIR/openscad_references"
TMP_STL="$(mktemp /tmp/openscad_ref_XXXXXX.stl)"
MAX_TIME=60  # seconds per file

trap 'rm -f "$TMP_STL"' EXIT

if ! command -v "$OPENSCAD" &>/dev/null; then
    echo "ERROR: openscad not found at $OPENSCAD" >&2
    exit 1
fi

count=0
skipped=0

for scad_file in $(find "$EXAMPLES_DIR" -name '*.scad' -type f | sort); do
    # Relative path from examples dir (e.g. "Basics/CSG.scad")
    relative="${scad_file#$EXAMPLES_DIR/}"
    # Output JSON path (e.g. tests/openscad_references/Basics/CSG.json)
    name_no_ext="${relative%.scad}"
    out_json="$REF_DIR/${name_no_ext}.json"

    echo -n "Processing $relative ... "

    # Run OpenSCAD with --summary all (with timeout to avoid hangs)
    output=$(timeout "$MAX_TIME" "$OPENSCAD" --summary all -o "$TMP_STL" "$scad_file" 2>&1)
    rc=$?
    if [[ $rc -ne 0 ]]; then
        echo "SKIPPED (openscad exit code $rc)"
        skipped=$((skipped + 1))
        continue
    fi

    # Extract vertices — may be missing for PolySet objects
    vertices=$(echo "$output" | grep -E '^\s*Vertices:' | awk '{print $2}')
    facets=$(echo "$output" | grep -E '^\s*Facets:' | awk '{print $2}')

    # Extract bounding box min/max
    bb_min=$(echo "$output" | grep -E '^\s*Min:' | sed 's/.*Min:[[:space:]]*//')
    bb_max=$(echo "$output" | grep -E '^\s*Max:' | sed 's/.*Max:[[:space:]]*//')

    # Skip if we didn't get bounding box data
    if [[ -z "$facets" || -z "$bb_min" || -z "$bb_max" ]]; then
        echo "SKIPPED (no valid geometry output)"
        skipped=$((skipped + 1))
        continue
    fi

    # Skip if facets is 0 (no geometry)
    if [[ "$facets" == "0" ]]; then
        echo "SKIPPED (0 facets)"
        skipped=$((skipped + 1))
        continue
    fi

    # Default vertices to 0 if not reported (PolySet objects)
    vertices="${vertices:-0}"

    # Parse bounding box coordinates
    min_x=$(echo "$bb_min" | cut -d',' -f1 | tr -d ' ')
    min_y=$(echo "$bb_min" | cut -d',' -f2 | tr -d ' ')
    min_z=$(echo "$bb_min" | cut -d',' -f3 | tr -d ' ')
    max_x=$(echo "$bb_max" | cut -d',' -f1 | tr -d ' ')
    max_y=$(echo "$bb_max" | cut -d',' -f2 | tr -d ' ')
    max_z=$(echo "$bb_max" | cut -d',' -f3 | tr -d ' ')

    # Create output directory
    mkdir -p "$(dirname "$out_json")"

    # Write JSON
    cat > "$out_json" <<EOF
{
  "vertices": $vertices,
  "facets": $facets,
  "bounding_box": {
    "min": [$min_x, $min_y, $min_z],
    "max": [$max_x, $max_y, $max_z]
  }
}
EOF

    echo "OK (vertices=$vertices, facets=$facets)"
    count=$((count + 1))
done

echo ""
echo "Done. Generated $count reference files, skipped $skipped."
