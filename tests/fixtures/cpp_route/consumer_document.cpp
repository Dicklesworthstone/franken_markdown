#include <iostream>
#include <vector>
#include <string>
#include <memory>
#include <optional>

#define BUFFER_SIZE 4096
#define MAX_ITEMS 1'000'000

#ifdef __cpp_concepts
template<typename T>
concept Printable = requires(T a) {
    std::cout << a;
};
#endif

namespace fcb::core {

// Base representation of a syntax node
class AstNode {
public:
    virtual ~AstNode() = default;
    virtual void dump(std::ostream& os) const = 0;
};

/* Multi-line comment documenting
   the Processor class and its
   raw string capabilities */
class Processor : public AstNode {
private:
    std::string name_;
    std::vector<int> values_;
    unsigned long long counter_{42ULL};
    double factor_{0x1.fp3};
    float scale_{3.14159f};
    int flags_{0b1010'0101};

public:
    explicit Processor(std::string name)
        : name_(std::move(name)) {
        values_.reserve(16);
    }

    void dump(std::ostream& os) const override {
        // Raw string literal with custom delimiter and quotes inside
        const char* query = R"query(
            SELECT id, name, payload
            FROM entries
            WHERE status = "active"
              AND flags != 0
        )query";

        // Encoding-prefixed literals
        const char8_t* u8msg = u8"UTF-8 encoded message";
        const wchar_t* wmsg = L"Wide string message";
        char8_t u8ch = u8'A';
        wchar_t wch = L'Ω';

        os << name_ << ": " << query << "\n";
        os << (counter_ <=> 0ULL == 0 ? "zero" : "positive") << "\n";
    }

    [[nodiscard]] auto compute(int a, int b) const -> std::optional<double> {
        if (b == 0 || a < 0) {
            return std::nullopt;
        }
        return static_cast<double>(a) / static_cast<double>(b) * factor_;
    }
};

} // namespace fcb::core

int main() {
    auto proc = std::make_unique<fcb::core::Processor>("test_runner");
    proc->dump(std::cout);
    auto res = proc->compute(100, 2);
    if (res.has_value()) {
        std::cout << "Result: " << *res << "\n";
    }
    return 0;
}
