# Makefile for Duktape application

CURRENT_DIR := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))

TOCK_USERLAND_BASE_DIR = $(abspath $(CURRENT_DIR)/../..)
TOCK_BASE_DIR = $(abspath $(CURRENT_DIR)/../../../)
BUILDDIR ?= $(abspath build/$(TOCK_ARCH))

C_SRCS := $(wildcard *.c)
OBJS += $(patsubst %.c,$(BUILDDIR)/%.o,$(C_SRCS))

TINY_EDUK_FLAGS = -DDUK_OPT_NO_FILE_IO -DDUK_OPT_LIGHTFUNC_BUILTINS -DDUK_OPT_BUFLEN16 -DDUK_OPT_OBJSIZES16 -DDUK_OPT_REFCOUNT16 -DDUK_OPT_STRHASH16 -DDUK_OPT_STRLEN16 -DDUK_OPT_SELF_TESTS -DDUK_OPT_SEGFAULT_ON_PANIC

CPPFLAGS += -DSTACK_SIZE=2048 $(TINY_EDUK_FLAGS)

# .h files that contain the contents of .js files
JS_H_FILES = \
	c_eventloop.h \
	duktapeHost.h \
	commonHost.h \
	events.h \
	util.h \
	ecma_eventloop.h \
	RampJSDisplay.h \
	RampJSTest.h \
	RampJSTestDisplay.h \
	Stop.h \
	autoTestComposite.h \
	autoTestStop.h \
	testCommon.h \
	TestAdder.h \
	TestComposite.h \
	TestDisplay.h \
	TestGain.h \
	TestSpontaneous.h \
	TrainableTest.h

# include userland master makefile. Contains rules and flags for actually
# 	building the application
include $(TOCK_USERLAND_BASE_DIR)/Makefile

.PHONY:
clean::
	rm -Rf $(BUILDDIR)

realclean:
	rm -rf accessors js2h node_modules *.[ch]

ACCESSORS = accessors

$(ACCESSORS):
	if [ ! -d $(ACCESSORS) ]; then \
		echo "Try checking out the accessors repo read/write"; \
		svn co https://repo.eecs.berkeley.edu/svn/projects/terraswarm/accessors/trunk/accessors; \
		if [ ! -d $(ACCESSORS) ]; then \
			echo "Checking out the accessors repo read/write, failed, checking it out read-only instead."; \
			svn co https://repo.eecs.berkeley.edu/svn-anon/projects/terraswarm/accessors/trunk/accessors; \
		fi; \
	else \
		(cd $(ACCESSORS); svn update); \
	fi

ACCESSORS_COMMON = $(ACCESSORS)/web/hosts/common
ACCESSORS_DUKTAPE = $(ACCESSORS)/web/hosts/duktape
ACCESSORS_TEST = $(ACCESSORS)/web/test

update: $(ACCESSORS)
	(cd $(ACCESSORS_DUKTAPE)/eduk; $(MAKE))
	cp $(ACCESSORS_DUKTAPE)/eduk/*.[ch] .
	cp $(ACCESSORS_DUKTAPE)/duktape/src/*.[ch] .
	@echo "The next step is to run \"make\""
