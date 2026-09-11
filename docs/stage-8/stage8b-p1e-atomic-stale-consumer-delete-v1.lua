local consumers = redis.call('XINFO', 'CONSUMERS', KEYS[1], ARGV[1])
local target_name = ARGV[2]
local minimum_idle = tonumber(ARGV[3])

for _, consumer in ipairs(consumers) do
    local name = nil
    local pending = nil
    local idle = nil
    for index = 1, #consumer, 2 do
        if consumer[index] == 'name' then
            name = consumer[index + 1]
        elseif consumer[index] == 'pending' then
            pending = tonumber(consumer[index + 1])
        elseif consumer[index] == 'idle' then
            idle = tonumber(consumer[index + 1])
        end
    end

    if name == target_name then
        if pending == nil or idle == nil or pending ~= 0 or idle < minimum_idle then
            return 0
        end
        local removed_pending = redis.call(
            'XGROUP', 'DELCONSUMER', KEYS[1], ARGV[1], target_name
        )
        if removed_pending ~= 0 then
            return redis.error_reply('pending changed inside atomic stale-consumer cleanup')
        end
        return 1
    end
end

return 0
