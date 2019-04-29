PREFIX = /usr/local

HEADER = rcf_cuckoofilter.h
LIBS = libcuckoofilter_cabi.a libcuckoofilter_cabi.so

error:
	@echo "Please use 'cargo build --release' for building, the Makefile is for installation only (via 'make install')."

.PHONY: install
install:
	install -D -m 0755 $(addprefix target/include/,$(HEADER)) -t $(DESTDIR)$(PREFIX)/include
	install -D -m 0755 $(addprefix target/release/,$(LIBS)) -t $(DESTDIR)$(PREFIX)/lib

.PHONY: uninstall
uninstall:
	rm -f $(DESTDIR)$(PREFIX)/lib/$(LIBS)
	rm -f $(DESTDIR)$(PREFIX)/include/$(HEADER)
