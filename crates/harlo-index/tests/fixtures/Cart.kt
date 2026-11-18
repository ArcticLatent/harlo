package shop

import kotlin.math.max

/** A cart. */
class Cart(val items: List<Item>) {
    fun total(tax: Int): Int = items.sum() + max(tax, 0)

    private fun sync() {
        Ledger().post(this)
    }

    companion object {
        fun empty() = Cart(listOf())
    }
}

object Registry {
    fun find(id: String): Cart? = null
}

interface Store { fun load() }

typealias Id = String

class CartTest {
    @Test
    fun totals() {}
}
