# CPU type on linuxy
CPUTYPE=`grep "model name" /proc/cpuinfo 2>/dev/null | uniq | cut -d':' -f2-`

if [ -z "${CPUTYPE}" ] ; then
    # CPU type on macos
    CPUTYPE=`sysctl -n machdep.cpu.brand_string 2>/dev/null`
fi

CPUTYPE="${CPUTYPE//[^[:alnum:]]/}"

OSTYPESTR="${OSTYPE//[^[:alnum:]]/}"

ARGS=$*
ARGSSTR="${ARGS//[^[:alnum:]]/}"

BNAME="cargo-bench"
FNAME="${BNAME}.result.${CPUTYPE}.${OSTYPESTR}.${ARGSSTR}.txt"
RESF="tmp/${FNAME}"

echo "# Saving result into \"${RESF}\""

rm -f $RESF
mkdir -p tmp

echo "# git log -1 | head -1" 2>&1 | tee -a $RESF
git log -1 | head -1 2>&1 | tee -a $RESF
echo 2>&1 | tee -a $RESF

echo "( [ -z \"\$(git status --porcelain)\" ] && echo \"Clean\" || echo \"Uncommitted changes\" )" 2>&1 | tee -a $RESF
( [ -z "$(git status --porcelain)" ] && echo "Clean" || echo "Uncommitted changes" ) 2>&1 | tee -a $RESF
echo 2>&1 | tee -a $RESF

echo CPU type: 2>&1 | tee -a $RESF
echo $CPUTYPE 2>&1 | tee -a $RESF
echo 2>&1 | tee -a $RESF

echo OS type: 2>&1 | tee -a $RESF
echo $OSTYPE 2>&1 | tee -a $RESF
echo 2>&1 | tee -a $RESF

if [ "x${OSTYPE}" = "xmsys" ]; then
	# no jemalloc on windows
	ALLOCATORS=mimalloc,rpmalloc,snmalloc
else
	ALLOCATORS=mimalloc,rpmalloc,jemalloc,snmalloc
fi

cargo --locked build --release --package bench --features=$ALLOCATORS 2>&1 | tee -a $RESF &&

echo "# ./target/release/bench --compare ${ARGS}" 2>&1 | tee -a $RESF &&
echo 2>&1 | tee -a $RESF &&

./target/release/bench --compare ${ARGS} 2>&1 | tee -a $RESF &&

echo "# Results are in \"${RESF}\" ."
