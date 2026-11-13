-- Nest test bed: a Text Controller exercising each kind of control the Nest
-- shows. Give the Text Controller a Code Name starting with NEST_ (e.g.
-- NEST_Test_Bed) and Script Access External, add these controls in its
-- control editor (each Count 1), then paste this script.
--
--   Name      Kind                          On the Nest
--   Lights    Button, Toggle                toggle (legend "Room Lights")
--   Source    Text, shown as a Combo Box    click to choose from a list
--   Level     Knob, in dB, -100 to 20       dial; click to adjust
--   Mute      Button, Toggle                mutes Level
--   Doorbell  Button, Trigger               button; rings Status for 3 s
--   Lock      Button, Toggle                disables Source and Level
--   Status    Indicator, Text               label, updated by the script
--   Occupied  Indicator, LED                indicator, blinks every 2 s
--   Extra     Button, Toggle                hidden and shown every 10 s
--
-- A control the Text Controller lacks is reported in the log and skipped.

local function control(name)
  local c = Controls[name]
  if c == nil then
    print("test bed: no control named " .. name .. "; add it in the control editor")
  end
  return c
end

local lights = control("Lights")
local source = control("Source")
local level = control("Level")
local mute = control("Mute")
local doorbell = control("Doorbell")
local lock = control("Lock")
local status = control("Status")
local occupied = control("Occupied")
local extra = control("Extra")

local function say(text)
  print("test bed: " .. text)
  if status then status.String = text end
end

-- A legend, to see whether the Nest shows it in place of the name.
if lights then
  lights.Legend = "Room Lights"
  lights.EventHandler = function(c)
    say("Lights " .. (c.Boolean and "on" or "off"))
  end
end

if source then
  source.Choices = { "Mic 1", "Mic 2", "Laptop", "Streaming", "Radio" }
  if source.String == "" then source.String = "Laptop" end
  source.EventHandler = function(c) say("Source: " .. c.String) end
end

if level then
  level.EventHandler = function(c) say("Level " .. c.String) end
end

if mute then
  mute.EventHandler = function(c) say(c.Boolean and "Muted" or "Unmuted") end
end

if doorbell then
  doorbell.EventHandler = function()
    say("Ding dong!")
    Timer.CallAfter(function() say("Ready") end, 3)
  end
end

-- Lock: the Nest leaves disabled controls off its screen.
local function apply_lock()
  local locked = lock and lock.Boolean or false
  if source then source.IsDisabled = locked end
  if level then level.IsDisabled = locked end
end
if lock then
  lock.EventHandler = function(c)
    apply_lock()
    say(c.Boolean and "Locked: Source and Level hidden" or "Unlocked")
  end
end
apply_lock()

-- Occupied blinks, to watch an indicator follow the Core.
if occupied then
  local blink = Timer.New()
  blink.EventHandler = function() occupied.Boolean = not occupied.Boolean end
  blink:Start(2)
end

-- Extra comes and goes, to watch the Nest skip an invisible control.
if extra then
  local shown = true
  local flip = Timer.New()
  flip.EventHandler = function()
    shown = not shown
    extra.IsInvisible = not shown
  end
  flip:Start(10)
end

say("Ready")
