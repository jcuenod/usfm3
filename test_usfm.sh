#!/bin/bash
# Test all SFM files from Paratext projects through rsusfm3
BINARY="/home/jcuenod/Programming/rsusfm3/target/release/usfm3"
OPTIONS=""
RESULTS_DIR="/tmp/usfm_test_results"
mkdir -p "$RESULTS_DIR"

# Find all SFM files (not ptxprint, modules, or resources)
find "/mnt/c/My Paratext 9 Projects" -maxdepth 2 \( -name "*.SFM" -o -name "*.sfm" -o -name "*.usfm" \) \
  -not -path "*/shared/*" -not -path "*/Modules/*" -not -path "*/_Resources/*" \
  | sort > "$RESULTS_DIR/all_files.txt"

echo "Total files: $(wc -l < "$RESULTS_DIR/all_files.txt")"

# Initialize counters
total_size=0
total_time_ns=0

# Test each file
> "$RESULTS_DIR/crash_files.txt"
> "$RESULTS_DIR/error_files.txt"  
> "$RESULTS_DIR/clean_files.txt"
> "$RESULTS_DIR/all_errors.txt"

while IFS= read -r file; do
  # Get relative path for display
  rel="${file#/mnt/c/My Paratext 9 Projects/}"
  
  # Run parser, capture stdout and stderr separately
  file_size=$(stat -c%s "$file")
  start_time=$(date +%s%N)
  stdout=$("$BINARY" vref $OPTIONS "$file" 2>"$RESULTS_DIR/stderr_tmp.txt")
  exit_code=$?
  end_time=$(date +%s%N)

  duration=$((end_time - start_time))
  total_size=$((total_size + file_size))
  total_time_ns=$((total_time_ns + duration))
  
  stderr=$(cat "$RESULTS_DIR/stderr_tmp.txt")
  
  if [ $exit_code -ne 0 ]; then
    echo "$rel" >> "$RESULTS_DIR/crash_files.txt"
    echo "=== CRASH: $rel (exit=$exit_code) ===" >> "$RESULTS_DIR/all_errors.txt"
    echo "$stderr" >> "$RESULTS_DIR/all_errors.txt"
    echo "" >> "$RESULTS_DIR/all_errors.txt"
  elif [ -n "$stderr" ]; then
    echo "$rel" >> "$RESULTS_DIR/error_files.txt"
    echo "=== ERRORS: $rel ===" >> "$RESULTS_DIR/all_errors.txt"
    echo "$stderr" >> "$RESULTS_DIR/all_errors.txt"
    echo "" >> "$RESULTS_DIR/all_errors.txt"
  else
    echo "$rel" >> "$RESULTS_DIR/clean_files.txt"
  fi
done < "$RESULTS_DIR/all_files.txt"

echo ""
echo "=== SUMMARY ==="
echo "Crashes: $(wc -l < "$RESULTS_DIR/crash_files.txt")"
echo "Files with errors: $(wc -l < "$RESULTS_DIR/error_files.txt")"
echo "Clean files: $(wc -l < "$RESULTS_DIR/clean_files.txt")"

# Print timing statistics
if [ $total_time_ns -gt 0 ]; then
  total_time_s=$(awk "BEGIN {print $total_time_ns / 1000000000}")
  total_mb=$(awk "BEGIN {print $total_size / 1048576}")
  mb_per_s=$(awk "BEGIN {print $total_mb / ($total_time_ns / 1000000000)}")
  
  echo ""
  printf "Total Size:    %.2f MB\n" "$total_mb"
  printf "Total Time:    %.3f s\n" "$total_time_s"
  printf "Average Speed: %.2f MB/s\n" "$mb_per_s"
fi
