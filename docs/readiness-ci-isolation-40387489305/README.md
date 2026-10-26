# User-agent shutdown verification

Run 40387489305 ended before build or workload. Both disable and bootout commands returned zero for both GUI agents. helpd immediately disappeared; corespotlightd remained discoverable in the immediate query. No OS protection refusal was observed; the generic script error included that possibility but is not evidence of it. The artifact preserves this distinction.

A bootout command can return before the remaining process has exited. The next verification records state/PID and checks removal for at most ten seconds after the single shutdown request. It does not repeat shutdown, kill arbitrary processes, bypass OS protections, or accept a still-present service. If removal is not observed, it fails with the shutdown evidence. This checks service lifecycle, not waiting for a performance test to turn green.
