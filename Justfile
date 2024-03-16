distdir := 'dist'

dist: manpage
	cargo build --release
	install -D target/release/rdev "{{distdir}}/bin/rdev"
	@echo "Build finished!"

clean:
	rm -rf target
	rm -rf {{distdir}}

_mkbuild:
	mkdir -p {{distdir}}

showpage:
	cd doc/ &&\
	scdoc < rdev.1.scdoc | man -l -

manpage: _mkbuild
	#!/usr/bin/env sh
	set -euxo pipefail
	destpath="{{distdir}}/share/man1/rdev.1.gz"
	install -d "$(dirname $destpath)"
	scdoc < doc/rdev.1.scdoc | gzip > "$destpath"
