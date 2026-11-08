// Independently authored fault injection into the exact production C ABI source.
// Interpose only OCIO's version/setter in this test translation unit. There are
// no test switches, hooks or environment overrides in the installed bridge.
#include <OpenColorIO/OpenColorIO.h>
#include <cstdio>
#include <cstring>
#include <new>
#include <stdexcept>
#include <string>

static int fault = 0, setter_calls = 0;
namespace OCIO_NAMESPACE {
static int motion_test_version() {
    return fault == 1 ? 0x02050000 : GetVersionHex();
}
static void motion_test_logging(LoggingFunction callback) {
    ++setter_calls;
    switch (fault) {
    case 2: throw Exception("controlled native OCIO initialization failure");
    case 3: throw std::bad_alloc();
    case 4: throw std::runtime_error("controlled standard initialization failure");
    case 5: throw 17;
    case 6: throw std::runtime_error(std::string("original initialization cause: ") + std::string(8192, 'x'));
    default: SetLoggingFunction(callback);
    }
}
}
#define GetVersionHex motion_test_version
#define SetLoggingFunction motion_test_logging
#include "bridge.cpp"
#undef GetVersionHex
#undef SetLoggingFunction

static void callback(const char *, size_t) {}
static bool require(bool value, const char *description) {
    if (!value) std::fprintf(stderr, "native OCIO boundary assertion failed: %s\n", description);
    return value;
}
int main() {
    Error error{};
    // Before any successful registration, failures must leave the callback
    // unpublished and preserve their category/cause instead of terminating.
    const uint32_t categories[] = {0, 1, 2, 3, 4, 5, 4};
    const char *causes[] = {"", "loaded OCIO is not 2.6.0", "controlled native OCIO initialization failure",
        "", "controlled standard initialization failure", "unknown native OCIO exception", "original initialization cause: "};
    for (int selected = 1; selected <= 6; ++selected) {
        fault = selected;
        setter_calls = 0;
        if (!require(!motion_ocio_initialize(callback, &error), "injected initialization must fail") ||
            !require(error.kind == categories[selected], "original exception category") ||
            !require(error.message[0] != 0 && error.message[2047] == 0, "bounded terminated cause") ||
            !require(std::strncmp(error.message, causes[selected], std::strlen(causes[selected])) == 0, "original cause") ||
            !require(logger.load() == nullptr, "failed callback must not be published") ||
            !require(setter_calls == (selected == 1 ? 0 : 1), "version admission precedes setter")) return 1;
        if (selected == 6 && !require(std::strlen(error.message) == 2047, "long original cause is bounded")) return 1;
        std::printf("initialize fault=%d category=%u cause_bytes=%zu callback_unpublished=true\n",
            selected, error.kind, std::strlen(error.message));
    }
    fault = 0;
    setter_calls = 0;
    if (!require(!motion_ocio_initialize(nullptr, &error), "null callback refused") ||
        !require(error.kind == 1 && setter_calls == 0 && logger.load() == nullptr, "null callback cannot reach native setter")) return 1;
    if (!require(motion_ocio_initialize(callback, &error), "healthy initialization recovers") ||
        !require(error.kind == 0 && error.message[0] == 0 && logger.load() == callback, "success clears error and publishes callback")) return 1;
    std::puts("initialize null_callback=refused recovery=success error_cleared=true");
    Program program{};
    program.catalogue = catalogue(ocio::Config::CreateRaw());
    CatalogueRow row{}; uint32_t count = 0;
    if (!require(motion_ocio_catalogue_row(&program, 0, &row, &count, &error), "actual native catalogue row") ||
        !require(count == program.catalogue.size() && row.name && row.value, "catalogue borrowed strings and count")) return 1;
    if (!require(!motion_ocio_catalogue_row(&program, count, &row, &count, &error), "out-of-range row refused") ||
        !require(error.kind == 1 && count == 0 && row.name == nullptr && row.value == nullptr, "failed catalogue query publishes no partial row")) return 1;
    if (!require(!motion_ocio_catalogue_row(nullptr, 0, &row, &count, &error), "null program refused") ||
        !require(error.kind == 1 && count == 0 && row.name == nullptr, "null program preserves bounded native error")) return 1;
    std::puts("catalogue actual_config=passed invalid_index=refused null_program=refused partial_output=false");
    // Do not leave a std::function manager referring to the independently
    // compiled test translation unit during the core library's final teardown.
    ocio::ResetToDefaultLoggingFunction();
    return 0;
}
