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

FNAME="cargo-bench.result.${CPUTYPE}.${OSTYPESTR}.${ARGSSTR}.txt"
TMPF="tmp/${FNAME}"
RESF="bench/results/${FNAME}"

echo "# Saving result into a tmp file (in ./tmp) which will be moved to \"${RESF}\" when complete..."

rm -f $TMPF
mkdir -p tmp

git log -1 | head -1 2>&1 | tee -a $TMPF
echo "# git log -1 | head -1" 2>&1 | tee -a $TMPF
echo 2>&1 | tee -a $TMPF

echo "[ -z \"\$(git status --porcelain)\" ] && echo \"Clean\" || echo \"Uncommitted changes\"" 2>&1 | tee -a $TMPF
[ -z "$(git status --porcelain)" ] && echo "Clean" || echo "Uncommitted changes" 2>&1 | tee -a $TMPF
echo 2>&1 | tee -a $TMPF

echo CPU type: 2>&1 | tee $TMPF
echo $CPUTYPE 2>&1 | tee $TMPF
echo 2>&1 | tee $TMPF

echo OS type: 2>&1 | tee -a $TMPF
echo $OSTYPE 2>&1 | tee -a $TMPF
echo 2>&1 | tee -a $TMPF

if [ "x${OSTYPE}" = "xmsys" ]; then
	# no jemalloc on windows
	ALLOCATORS=mimalloc,rpmalloc,snmalloc
else
	ALLOCATORS=mimalloc,rpmalloc,jemalloc,snmalloc
fi

cargo --locked build --release --package bench --features=$ALLOCATORS 2>&1 | tee -a $TMPF

echo "# ./target/release/bench --compare ${ARGS}" 2>&1 | tee -a $TMPF
echo 2>&1 | tee -a $TMPF

./target/release/bench --compare ${ARGS} 2>&1 | tee -a $TMPF

mv -f "${TMPF}" "${RESF}"
