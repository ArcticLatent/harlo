require "json"
require_relative "item"

# A cart of items.
module Shop
  class Cart
    # The total.
    def total(tax)
      items.sum(tax) + helper(1)
    end

    def self.build
      new
    end

    private

    def sync
      Ledger.new.post(self)
    end
  end
end
