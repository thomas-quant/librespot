// Diagnostic only: the historical algorithm is NOT established compatible with v5.
// No table, token, account, response or content key is embedded or logged.
#include <array>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <sys/resource.h>
#include <sys/prctl.h>
#include "main.h"

#if !defined(__linux__) || !defined(__x86_64__)
#error "This bounded diagnostic worker supports Linux x86_64 only"
#endif
static_assert(sizeof(uint32) == 4 && sizeof(uint64) == 8);
uint32 playIntentKey[768];

int main() {
    const rlimit core{0, 0}, cpu{5, 5}, memory{256 * 1024 * 1024, 256 * 1024 * 1024};
    if (setrlimit(RLIMIT_CORE, &core) || setrlimit(RLIMIT_CPU, &cpu) ||
        setrlimit(RLIMIT_AS, &memory) || prctl(PR_SET_DUMPABLE, 0, 0, 0, 0)) return 7;
    std::array<uint8_t, 8 + 3072 + 16 + 20> input{};
    std::cin.read(reinterpret_cast<char *>(input.data()), input.size());
    if (std::cin.gcount() != static_cast<std::streamsize>(input.size()) ||
        std::cin.peek() != std::char_traits<char>::eof() ||
        std::memcmp(input.data(), "LPPCAN01", 8)) return 2;
    size_t offset = 8;
    for (auto &word : playIntentKey) {
        word = 0;
        for (unsigned shift = 0; shift < 32; shift += 8)
            word |= uint32(input[offset++]) << shift;
    }
    std::array<uint8_t, 32> output{};
    // First candidate: exact old decrypt + file-ID binding.
    // Second: same intermediate without binding, a separately named hypothesis.
    decrypt_main(input.data() + offset, output.data() + 16);
    bind_key(output.data() + 16, input.data() + offset + 16, output.data());
    std::cout.write(reinterpret_cast<const char *>(output.data()), output.size());
    std::cout.flush();
    return std::cout ? 0 : 3;
}
