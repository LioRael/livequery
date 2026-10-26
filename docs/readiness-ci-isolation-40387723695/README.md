# User indexing agents removed; admission still fails

Run 40387723695 on 65422cc. Both user agents were successfully disabled and removed. corespotlightd transitioned from SIGTERMed to absent after one second; the earlier immediate-query failure was a lifecycle race, not an OS protection denial.

After build and the unchanged fixed 180-second settling interval, all four admission samples failed: load1 4.425, 4.311, 4.446, 5.691; aggregate external ps CPU 132.4%, 255.9%, 239.6%, 186.9%. Activity included mobileassetd, mlhostd, secinitd, iconservicesagent, triald, trustd, photoanalysisd and softwareupdated. The workload never started. Removing observed indexing agents is therefore insufficient for this hosted image. Do not continue disabling security or other OS services opportunistically.

The next diagnostic comparison selects the documented macos-15 ARM hosted image explicitly, preserving the same indexing isolation, admission limits and workload parameters. This changes OS/image and must be labeled an environment comparison, not a fix proven on macos-26 or an automatic replacement of final acceptance. The production CI/endurance runner labels remain unchanged. All previous failures remain retained. If this comparison passes, macOS 26 coverage and the final accepted environment still require an explicit evidence-based decision.

Runner label reference: https://docs.github.com/en/actions/how-tos/write-workflows/choose-where-workflows-run/choose-the-runner-for-a-job
