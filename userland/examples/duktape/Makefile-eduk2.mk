# Makefile for a Duktape 2.x accessor host
#
# make -f Makefile-eduk2.mk TOCK_BOARD=hail program
#

# Specify this directory relative to the current application.
TOCK_USERLAND_BASE_DIR = ../..

# Which files to compile.
C_SRCS := duktape.c \
	duk_stack.c \
	c_eventloop.c \
	eduk2.c \
	modSearch.c \
	nofileio.c

# HAIL_PRINT just calls printf()
#SWARMLET = -DHAIL_PRINT

# EDUK_PRINT invokes JavaScript print(), much like main-simple.c
SWARMLET = -DEDUK_PRINT

#SWARMLET = -DEDUK_MIN 
#SWARMLET = -DEDUK_MIN -DHAIL_PRINT

CFLAGS += -Os -DSTACK_SIZE=6134 -DAPP_HEAP_SIZE=32768 $(SWARMLET)

# Include userland master makefile. Contains rules and flags for actually
# building the application.
include $(TOCK_USERLAND_BASE_DIR)/Makefile

# Standalone host binary (not TockOS/Hail)
# To compile, use:
#     make -f Makefile-eduk2.mk eduk2
eduk2: $(C_SRCS) 
	gcc -o $@ $(C_SRCS) -I. -I$(EDUK) -Os


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
	#(cd $(ACCESSORS_DUKTAPE)/eduk2; $(MAKE) clean)
	(cd $(ACCESSORS_DUKTAPE)/eduk2; $(MAKE) EDUK_DEFINES="$(SWARMLET)")
	cp $(ACCESSORS_DUKTAPE)/eduk2/*.[ch] .
	@echo "Optional: To run the C version, run: ./accessors/web/hosts/duktape/eduk/eduk"
	$(MAKE) clean
	@echo "The next step is to run make TOCK_BOARD=hail program"
