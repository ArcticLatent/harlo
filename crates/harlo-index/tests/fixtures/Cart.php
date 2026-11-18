<?php
namespace App\Models;

use App\Ledger;
require_once 'helpers.php';

/** A cart. */
class Cart extends Model
{
    public function total(int $tax): int
    {
        return $this->items->sum() + max($tax, 0);
    }

    private function sync()
    {
        Ledger::post($this);
        $l = new Ledger();
    }
}

function helper() {}

interface Store { public function load(); }
trait Countable {}
