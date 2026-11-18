package shop;

import java.util.List;

/** A cart of items. */
public class Cart {
    private final List<Item> items;

    public Cart(List<Item> items) {
        this.items = items;
    }

    /** The total, with tax. */
    public int total(int tax) {
        return items.stream().count() + Math.max(tax, 0);
    }

    private void sync() {
        new Ledger().post(this);
    }
}

interface Store {
    void load();
}

enum Mode { A, B }

record Point(int x, int y) {}

class CartTest {
    @Test
    void totals() { }
}
