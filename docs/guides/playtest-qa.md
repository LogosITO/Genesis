# The Passage — manual release QA

Record date, Windows build, GPU, driver, backend from console, build SHA, ZIP SHA256, tester, result, and any error or screenshot for each run. Run from an extracted ZIP in a directory outside the source tree whose path contains spaces. Do not mark a step PASS from the headless test alone.

| Step | Expected observation | Result / evidence |
| --- | --- | --- |
| Launch `PLAY.cmd` | Window and readable overlay; no missing files or GPU error | |
| Read overlay without guide | Player, goal, movement, selection, source and cut are understandable | |
| Move with WASD | Orange player moves and wall or green stem blocks the lane | |
| Orbit with arrows, then click green node or connection | Selection names a node or branch; click after camera movement still targets what is visible | |
| Click blue growth, wall and empty space | Blue cut does not claim green path cleared; invalid targets give feedback | |
| Press P on lower green stem | Stem disappears on a simulation tick; collision route opens | |
| Press M | Yellow source moves; overlay changes to AWAY; later growth changes | |
| Enter cleared passage and reach goal | Victory message appears only after source move and stem cut | |
| Press R after partial progress and after victory | Original state, position, source and controls return | |
| Press Space twice | World pauses and resumes; queued actions wait while paused | |
| Hold movement, switch window, release key, return | Player does not continue moving without a new key press | |
| Press Escape | Application exits normally | |

Repeat the launch and interaction checks on at least one Windows machine other than the development machine before external distribution. Attach exact failures to the research record; do not infer broad GPU support from one adapter.
