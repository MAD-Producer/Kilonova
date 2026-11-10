// Qualification-only DLL. Never installed or included in runtime payloads.
// Inject a real bad_alloc at OCIO callback registration; all exported C ABI
// functions and their exception handling come from the unchanged product file.
#include <OpenColorIO/OpenColorIO.h>
#include <new>
namespace OCIO_NAMESPACE {
static void motion_test_failed_logging(LoggingFunction) { throw std::bad_alloc(); }
}
#define SetLoggingFunction motion_test_failed_logging
#include "bridge.cpp"
#undef SetLoggingFunction
