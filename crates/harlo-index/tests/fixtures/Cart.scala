package shop

import scala.math.max

/** A cart. */
class Cart(items: List[Int]) {
  def total(tax: Int): Int = items.sum + max(tax, 0)

  private def sync(): Unit = {
    Ledger.post(this)
  }
}

object Cart {
  def empty: Cart = new Cart(Nil)
}

trait Store {
  def load(): Unit
}

type Id = String
