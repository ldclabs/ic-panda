<script lang="ts">
  interface Props {
    addEmoji?: (emoji: string) => void
  }

  let { addEmoji = () => {} }: Props = $props()

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

  let emojis: string[] = $state([])
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
        onpointerenter={() => chooseEmojiSet(i)}
        onclick={() => chooseEmojiSet(i)}
      >
        {icon}
      </button>
    {/each}
  </header>

  <div class="flex h-60 flex-row flex-wrap overflow-y-auto">
    {#each emojis as emoji}
      <button class="btn p-2 text-2xl" onclick={onClickEmoji}>{emoji}</button>
    {/each}
  </div>
</section>
