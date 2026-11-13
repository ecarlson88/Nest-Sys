-- Combo box for testing the Nest: paste into a Text Controller whose Code
-- Name starts with NEST_ (any case) and Script Access is External or All.
--
-- The Text Controller needs one Text control (Designer's default,
-- "Control" with count 1, is seen over QRC as "Control"). To see the list
-- in Designer, show that control as a Combo Box.

local choices = { "Mic 1", "Mic 2", "Laptop", "Streaming" }

-- A single control is Controls.Control; a group of them is a table.
local combo = Controls.Control
if type(combo) == "table" then
  combo = combo[1]
end

combo.Choices = choices
if combo.String == "" then
  combo.String = choices[1]
end

combo.EventHandler = function(ctl)
  print("NEST combo: " .. ctl.String)
end
