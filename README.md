<img src="src/assets/readme/readme-heroimg.png" alt="Rice Cooker hero image">

## rice cooker
a visual (toy) tool for ricing hyprland.

browse rices, try them live on your desktop, and install them with a keyboard click. if you don't like what you see, _revert_ returns you to the original rice you started with.

rice cooker only points quickshell at your selected rice. it does not overwrite your existing files or directories, and does not modify your dotfiles.

https://github.com/user-attachments/assets/e5a63109-de89-48cc-a703-f249222df6f6

## notice

rice cooker currently only works on arch + hyprland and only contains quickshell rices. close any non-quickshell rice when using.

rice cooker's boot screen automatically rejects non-supported setups. forcible entry is possible but rice at your own risk.

## install

```bash
yay -S rice-cooker
# or paru
```

## user guide
rice cooker has two modes: (1) browse and (2) preview.
<img src="src/assets/readme/blueprint.png" alt="Rice Cooker interface blueprint showing browse and preview modes">



### 1. browse mode (expanded)

explore rices from the community.

- **1.1 menu (ESC)**
  - revert (hold ↵ to confirm)
  - submit a rice
  - credits
- **1.2 close** — exit rice cooker
- **1.3 sound** — toggle sound on and off
- **1.4 theme** — switch between 3 colour themes
- **1.5 HUD** — provides context like rice name, rice number, keyboard press indicators

### 2. preview mode (mini)

selecting a rice shows a live preview on your actual desktop. during this mode rice cooker collapses to give the rice room to breathe.

- **2.1 antenna** — extends when content is downloading
- **2.2 leave** — exit preview and revert to your original rice
- **2.3 install** — install the selected rice
- **2.4 dot** — opens the rice's dotfiles on github

preview and install times depend on the size of the rice. you may be prompted for a password when installing dependencies.

### rice cooker is best experienced with the keyboard and sound on.
- ↑   move selection up
- ↓   move selection down
- ↵   apply selection


## submitting a rice

the rice cooker catalog is built from dotfiles openly shared by the community. share a rice of your own with us (and everyone) through the link below.

a good rice is organized and complete. every submission is reviewed before it lands in the catalog.

[→ submit a rice](https://rumbling-turret-acd.notion.site/3502d6c6b1b280c6887ac95c786c2285?pvs=105)

## credits

made for fun by two brothers at butterfly.

[website](https://butterfly.so) &nbsp;|&nbsp; [x](https://x.com/bflycomputer) &nbsp;|&nbsp; [instagram](https://www.instagram.com/bflycomputer/)
