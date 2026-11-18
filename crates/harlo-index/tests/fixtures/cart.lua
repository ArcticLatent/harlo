local json = require("json")

local M = {}

--- The total.
function M.total(items, tax)
  return sum(items) + math.max(tax, 0)
end

function M:sync()
  self.ledger:post(self)
end

local function helper()
  print("x")
end

return M
