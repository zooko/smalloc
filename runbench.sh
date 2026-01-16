# CPU type on linuxy
CPUTYPE=`grep "model name" /proc/cpuinfo 2>/dev/null | uniq | cut -d':' -f2-`

if [ "x${CPUTYPE}" = "x" ] ; then
    # CPU type on macos
    CPUTYPE=`sysctl -n machdep.cpu.brand_string 2>/dev/null`
fi

CPUTYPE="${CPUTYPE//[^[:alnum:]]/}"

OSTYPESTR="${OSTYPE//[^[:alnum:]]/}"

ARGS=$*

ARGSSTR="${ARGS//[^[:alnum:]]/}"

RESF=bench/results/cargo-bench.result.${CPUTYPE}.${OSTYPESTR}.${ARGSSTR}.txt

echo "# Saving result into a file named \"${RESF}\" ..."

rm -f $RESF

git log -1 | head -1 2>&1 | tee -a $RESF
echo "# git log -1 | head -1" 2>&1 | tee -a $RESF
echo 2>&1 | tee -a $RESF

echo "[ -z \"\$(git status --porcelain)\" ] && echo \"Clean\" || echo \"Uncommitted changes\"" 2>&1 | tee -a $RESF
[ -z "$(git status --porcelain)" ] && echo "Clean" || echo "Uncommitted changes" 2>&1 | tee -a $RESF
echo 2>&1 | tee -a $RESF

echo CPU type: 2>&1 | tee $RESF
echo $CPUTYPE 2>&1 | tee $RESF
echo 2>&1 | tee $RESF

echo OS type: 2>&1 | tee -a $RESF
echo $OSTYPE 2>&1 | tee -a $RESF
echo 2>&1 | tee -a $RESF

if [ "x${OSTYPE}" = "xmsys" ]; then
	# no jemalloc on windows
	ALLOCATORS=mimalloc,rpmalloc,snmalloc
else
	ALLOCATORS=mimalloc,rpmalloc,jemalloc,snmalloc
fi

cargo --locked build --release --package bench --features=$ALLOCATORS 2>&1 | tee -a $RESF

echo "# ./target/release/bench --compare ${ARGS}" 2>&1 | tee -a $RESF
echo 2>&1 | tee -a $RESF

./target/release/bench --compare ${ARGS} 2>&1 | tee -a $RESF
