<script lang="ts">
  export let addEmoji: (emoji: string) => void = () => {}

  const setIcons = ['😃', '🤡', '👋', '🐼', '🚀', '🎁']
  const emojiSets = [
    {
      type: 'faces',
      set: [
        '😀',
        '😃',
        '😄',
        '😁',
        '😆',
        '😅',
        '😂',
        '🤣',
        '🥲',
        '🥹',
        '😊',
        '😇',
        '🙂',
        '🙃',
        '😉',
        '😌',
        '😍',
        '🥰',
        '😘',
        '😋',
        '😛',
        '😝',
        '😜',
        '🤪',
        '🤨',
        '🧐',
        '🤓',
        '😎',
        '🤩',
        '🥳',
        '🤗',
        '🫣',
        '🫡',
        '🤔',
        '🫠'
      ]
    },
    {
      type: 'faces2',
      set: [
        '😵‍💫',
        '😭',
        '🫨',
        '😮‍💨',
        '😤',
        '😠',
        '😡',
        '🤬',
        '🤯',
        '😬',
        '🥶',
        '😱',
        '😰',
        '🙄',
        '😴',
        '😵',
        '😵‍💫',
        '🤐',
        '🤮',
        '😷',
        '🤕',
        '🫥',
        '🤠',
        '🤡',
        '👹',
        '💩',
        '👻',
        '💀',
        '👽',
        '🤖'
      ]
    },
    {
      type: 'body',
      set: [
        '👋',
        '🤚',
        '✋',
        '👌',
        '🤌',
        '🤏',
        '🤙',
        '🫵',
        '🫱',
        '🫲',
        '👈',
        '👉',
        '👆',
        '👇',
        '👍',
        '👎',
        '✊',
        '👊',
        '🤛',
        '🤜',
        '👏',
        '🫶',
        '🙌',
        '🤝',
        '🙏',
        '💪',
        '🦾',
        '🦶',
        '👀',
        '👁',
        '🧠',
        '👅',
        '👄',
        '💋'
      ]
    },
    {
      type: 'animals',
      set: [
        '🐶',
        '🐱',
        '🐭',
        '🐹',
        '🐰',
        '🦊',
        '🐻',
        '🐼',
        '🐻‍❄️',
        '🐨',
        '🐯',
        '🦁',
        '🐮',
        '🐷',
        '🐸',
        '🐵',
        '🙈',
        '🐔',
        '🐦‍⬛',
        '🐣',
        '🐥',
        '🦆',
        '🦅',
        '🦉',
        '🦄',
        '🐝',
        '🐛',
        '🦋',
        '🐌',
        '🐞',
        '🦂',
        '🦖',
        '🦕',
        '🐙',
        '🦑',
        '🦐',
        '🦞',
        '🦀',
        '🪼',
        '🐳',
        '🐬'
      ]
    },
    {
      type: 'transport',
      set: [
        '🚀',
        '🚗',
        '🚕',
        '🚌',
        '🏎',
        '🚓',
        '🚑',
        '🚒',
        '🚜',
        '🦽',
        '🚲',
        '🛵',
        '🏍',
        '🚨',
        '🚔',
        '🚘',
        '🚇',
        '🚝',
        '🚂',
        '🛫',
        '🛸',
        '🚁',
        '⛵️',
        '🚢',
        '🚦',
        '🌎',
        '🪐',
        '☄️',
        '🔥',
        '🌈',
        '🌪'
      ]
    },
    {
      type: 'misc',
      set: [
        '⌚️',
        '📱',
        '💻',
        '🖥',
        '🖨',
        '🖲',
        '🕹',
        '💿',
        '📷',
        '🎥',
        '🎞',
        '☎️',
        '📻',
        '🎙',
        '🎛',
        '🧭',
        '⏱',
        '⏰',
        '⏳',
        '📡',
        '🔋',
        '🧯',
        '🛢',
        '🛍️',
        '💵',
        '💴',
        '🪙',
        '💰',
        '💳',
        '💎',
        '🪜',
        '🧰',
        '🪛',
        '🛠',
        '⚙️',
        '⛓',
        '⛓️‍💥',
        '🧲',
        '💣',
        '🧨',
        '⚔️',
        '🔮',
        '🧿',
        '💈',
        '🕳',
        '💊',
        '🩸',
        '🧬',
        '🔑',
        '🛒',
        '🎁',
        '🎈',
        '🎊',
        '🎉',
        '🧧',
        '🏮',
        '📦',
        '🔗',
        '📎',
        '🖇',
        '📐',
        '🧮',
        '📌',
        '📍',
        '✂️',
        '🔎',
        '🔍',
        '🔐',
        '🔓'
      ]
    }
  ]

  let emojis: string[] = []
  function chooseEmojiSet(id: number) {
    emojis = emojiSets[id]?.set || emojiSets[0]!.set
  }
  chooseEmojiSet(0)

  function onClickEmoji(e: any) {
    addEmoji(e.target.textContent)
  }
</script>

<section class="w-[264px]">
  <header class="border-b border-surface-500/20">
    {#each setIcons as icon, i}
      <button
        class="btn p-2 text-2xl"
        on:pointerenter={() => chooseEmojiSet(i)}
        on:click|stopPropagation={() => chooseEmojiSet(i)}
      >
        {icon}
      </button>
    {/each}
  </header>

  <div class="flex h-60 flex-row flex-wrap overflow-y-auto">
    {#each emojis as emoji}
      <button class="btn p-2 text-2xl" on:click={onClickEmoji}>{emoji}</button>
    {/each}
  </div>
</section>
