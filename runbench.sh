GLOBAL=smallocb_allocator_config_globalalloc.rs
SMALLOC=smallocb_allocator_config_smalloc.rs
LINKFILE=smallocb_allocator_config.rs

echo Baseline is Rust default global allocator, candidate is smalloc.

get_mtime() {
    if [[ "$OSTYPE" == "darwin"* ]] || [[ "$OSTYPE" == "freebsd"* ]]; then
        stat -f %m "$1"
    else
        stat -c %Y "$1"
    fi
}

maybe_touch() {
    MTIMEO=$(get_mtime "$1")
    MTIMEN=$(get_mtime "$2")
    if [ "$MTIMEO" -ge "$MTIMEN" ]; then
        touch "$2"
    fi
}

# If the file we're now linking to is not newer than the one we stopped linking to, then we need to
# touch it to force a (partial) rebuild.

mylink() {
    OLD=$(readlink "$LINKFILE") &&

    if [ "$OLD" != "$1" ]; then
        ln -snf "$1" "$LINKFILE" &&

        $(maybe_touch "$OLD" "$1")
    fi
}
    

cd src &&
    rm "${LINKFILE}" &&
    cp "${GLOBAL}" "${LINKFILE}" &&
cd .. &&
cargo --frozen export target/benchmarks -- bench --bench=smallocb &&
cd src &&
    rm "${LINKFILE}" &&
    cp "${SMALLOC}" "${LINKFILE}" &&
cd .. &&
time cargo --frozen bench -q --bench=smallocb -- compare target/benchmarks/smallocb &&
echo done   
