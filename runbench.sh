# CPU type on linuxy
CPUTYPE=`grep "model name" /proc/cpuinfo 2>/dev/null | uniq | cut -d':' -f2-`

if [ "x${CPUTYPE}" = "x" ] ; then
    # CPU type on macos
    CPUTYPE=`sysctl -n machdep.cpu.brand_string 2>/dev/null`
fi

CPUTYPE="${CPUTYPE//[^[:alnum:]]/}"

echo CPU type:
echo $CPUTYPE
echo

cargo build --release --package bench --features=mimalloc,rpmalloc,jemalloc,snmalloc

echo "# ./target/release/bench --compare 2>&1 | tee bench/results/cargo-bench.output.${CPUTYPE}.txt"
echo

./target/release/bench --compare 2>&1 | tee bench/results/cargo-bench.output.${CPUTYPE}.txt
