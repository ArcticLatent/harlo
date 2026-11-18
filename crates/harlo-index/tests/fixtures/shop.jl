module Shop

using LinearAlgebra
import Base: show

"""
A cart of items.
"""
struct Cart
    items::Vector{Int}
end

abstract type Store end

"The total, with tax."
function total(c::Cart, tax::Int)::Int
    return sum(c.items) + max(tax, 0)
end

area(r) = pi * r^2

macro twice(ex)
    :($ex; $ex)
end

function Base.show(io::IO, c::Cart)
    print(io, "Cart")
end

end
