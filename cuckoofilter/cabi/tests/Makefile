INCLUDE = ../target/include
LIB = ../target/release
CFLAGS = -I$(INCLUDE)
BUILDDIR = build

LDFLAGS_STATIC = -L$(LIB) -l:libcuckoofilter_cabi.a -ldl -lpthread
LDFLAGS_DYNAMIC = -L$(LIB) -Wl,-rpath=$(LIB) -lcuckoofilter_cabi


tests: basic_operations_static basic_operations_dynamic basic_operations_cpp_static basic_operations_cpp_dynamic


basic_operations_static: basic_operations.c
	mkdir -p $(BUILDDIR)
	$(CC) $(CFLAGS) -o $(BUILDDIR)/$@ $^ $(LDFLAGS_STATIC)

basic_operations_dynamic: basic_operations.c
	mkdir -p $(BUILDDIR)
	$(CC) $(CFLAGS) -o $(BUILDDIR)/$@ $^ $(LDFLAGS_DYNAMIC)

basic_operations_cpp_static: basic_operations.c
	mkdir -p $(BUILDDIR)
	$(CXX) $(CFLAGS) -o $(BUILDDIR)/$@ $^ $(LDFLAGS_STATIC)

basic_operations_cpp_dynamic: basic_operations.c
	mkdir -p $(BUILDDIR)
	$(CXX) $(CFLAGS) -o $(BUILDDIR)/$@ $^ $(LDFLAGS_DYNAMIC)

clean:
	rm basic_operations_static basic_operations_dynamic basic_operations_cpp_static basic_operations_cpp_dynamic
