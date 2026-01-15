# CPU type on linuxy
CPUTYPE=`grep "model name" /proc/cpuinfo 2>/dev/null | uniq | cut -d':' -f2-`

if [ "x${CPUTYPE}" = "x" ] ; then
    # CPU type on macos
    CPUTYPE=`sysctl -n machdep.cpu.brand_string 2>/dev/null`
fi

CPUTYPE="${CPUTYPE//[^[:alnum:]]/}"

ARGS=$*

ARGSSTR="${ARGS//[^[:alnum:]]/}"

LOGF=bench/results/cargo-bench.output.${CPUTYPE}.${ARGSSTR}.txt

echo "# Saving output into log file named \"${LOGF}\" ..."

echo CPU type: 2>&1 | tee $LOGF
echo $CPUTYPE 2>&1 | tee $LOGF
echo 2>&1 | tee $LOGF

cargo build --release --package bench --features=mimalloc,rpmalloc,jemalloc,snmalloc 2>&1 | tee $LOGF

echo "# ./target/release/bench --compare ${ARGS}" 2>&1 | tee $LOGF
echo 2>&1 | tee $LOGF

./target/release/bench --compare ${ARGS} 2>&1 | tee $LOGF
