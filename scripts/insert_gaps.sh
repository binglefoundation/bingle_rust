#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "Usage: $0 <file>" >&2
  exit 1
fi

file="$1"

if [ ! -f "$file" ]; then
  echo "Error: file not found: $file" >&2
  exit 1
fi

tmp_file="tmp/ts_gap.XXXXXX"

perl -MTime::Local -e '
  my $file = shift @ARGV;
  open my $in, q{<}, $file or die "Cannot open input: $!";

  my @out;
  my $prev;

  while (my $line = <$in>) {
    if ($line =~ /(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})\.(\d{3})/) {
      my ($Y,$M,$D,$h,$m,$s,$ms) = ($1,$2,$3,$4,$5,$6,$7);
      my $t = timelocal($s,$m,$h,$D,$M-1,$Y) + $ms/1000;

      push @out, "========\n" if defined $prev && ($t - $prev) >= 2;
      $prev = $t;
    }

    push @out, $line;
  }

  close $in;
  print @out;
' "$file" > "$tmp_file"

mv "$tmp_file" "$file"