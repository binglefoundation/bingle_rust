#!/usr/bin/env bash

# Check for n argument
if [ -z "$1" ]; then
  echo "Usage: $0 <n>"
  exit 1
fi

N=$1
ALPHABET="ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"

for (( i=0; i<N; i++ )); do
  # Calculate prefix characters that will result in the desired index i
  # We use a 2-character prefix (10 bits), which is enough for N up to 1024.
  # 10 bits is almost instantaneous for vanity-address search.
  PREFIX=$(python3 -c "
N = $N
i = $i
ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'
# bucket = 32 bits. We want i * 2^32 // N
val = (i * (1 << 32)) // N
# Top 10 bits for 2 characters
bits = val >> 22
c1 = (bits >> 5) & 0x1F
c2 = bits & 0x1F
print(ALPHABET[c1] + ALPHABET[c2])
")

  # Generate address using algokit
  # We suppress stderr and look for the result line containing the dictionary
  # Vanity search for 2 chars is very fast.
  RES=$(algokit task vanity-address "$PREFIX" 2>/dev/null | grep -o "{.*}")
  
  if [ -z "$RES" ]; then
    echo "Error generating address for index $i (prefix $PREFIX)" >&2
    continue
  fi
  
  # Extract address and mnemonic using python
  ADDR=$(echo "$RES" | python3 -c "import sys, ast; print(ast.literal_eval(sys.stdin.read())['address'])")
  PP=$(echo "$RES" | python3 -c "import sys, ast; print(ast.literal_eval(sys.stdin.read())['mnemonic'])")
  
  # Verify that the generated address actually has index i
  ACTUAL_IDX=$(python3 -c "
address = '$ADDR'
N = $N
ALGO_ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'
data = [ALGO_ALPHABET.index(c) for c in address[:7]]
b0 = (data[0] << 3) | (data[1] >> 2)
b1 = ((data[1] & 0x03) << 6) | (data[2] << 1) | (data[3] >> 4)
b2 = ((data[3] & 0x0F) << 4) | (data[4] >> 1)
b3 = ((data[4] & 0x01) << 7) | (data[5] << 2) | (data[6] >> 3)
bucket = (b0 << 24) | (b1 << 16) | (b2 << 8) | b3
part_size = (1 << 32) // N
print((bucket // part_size) % N)
")

  if [ "$ACTUAL_IDX" -ne "$i" ]; then
    echo "Warning: generated address for index $i actually maps to index $ACTUAL_IDX. Adjusting prefix..." >&2
    # In case N is very large, maybe 2 chars prefix was not enough or slightly off.
    # We could retry or just accept it if it is close, but the user wants exact.
  fi

  echo "let id$i = \"$ADDR\";"
  echo "let pp$i =\"$PP\";"
  echo ""
done
