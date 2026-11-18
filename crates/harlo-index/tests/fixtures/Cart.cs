using System;
using System.Linq;

namespace Shop
{
    /// <summary>A cart.</summary>
    public class Cart
    {
        public int Count { get; set; }

        public int Total(int tax)
        {
            return Items.Sum() + Math.Max(tax, 0);
        }

        private void Sync() { new Ledger().Post(this); }
    }

    interface IStore
    {
        void Load();
    }

    public class CartTests
    {
        [Fact]
        public void Totals() { }
    }
}
