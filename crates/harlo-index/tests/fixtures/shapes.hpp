#include <vector>

namespace geo {

/// A shape.
class Shape {
public:
    virtual double area() const { return 0; }
    void draw() { render(this); }
private:
    int id() { return 1; }
};

struct Box : Shape {
    double area() const override { return w * h; }
};

template <typename T>
T twice(T x) { return x * 2; }

using Id = int;

}

double geo::Shape::scale(double f) { return f; }
