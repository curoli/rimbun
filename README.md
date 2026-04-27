# Rimbun

Rimbun is a project to create a new kind of collaborative content creation platform, where differences between versions from different contributors can easily be seen and browsed, with the help of AI.

## Motivation

When you see a sentence on Wikipedia, all you know is that some one brought this sentence into its current form, and since then, no one chenged it. You can not see immediately whether there is strong disagreement to it.

Sure, you can browse the version history. But that means scanning all changes ever made, and there is no easy way to see, which changes affect this particular sentence.

With git, there is a feature to find out who made the last change to a line, and when, and one could look up the relevant commit and see what was before. But that requires considerable effort, too.

The idea of Rimbun is that as you see the text, you can immediately see where alternative versions exist, and you can browse them and compare them immediately without much effort.

To a degree, of course. When too many versions exist, not all can be shown. Also, a version might have been downvoted enough to be deemed unworthy to be shown, or excluded for other reasons. But where alternative versions exist with substantial community support, these version will absolutely be immediately visible.

To achieve this goal, Rimbun needs to be aware of two different kinds of similarity between versions: literal and semantic.

Literal similarity means, for example, two texts have identical segments. For example, two sentences where only one word differs. Literal similarity is essential when multiple versions are shown to readers for comparison.

Semantic similarity means, two versions have a similar meaning or impact. Semantic similarity can be detected through user feedback or through AI. Semantic similarity is essential when deciding which versions are prioritized for showing to readers.

For example, take these three sentences:

1. Today is a hot day in Bandung
1. Today is a cold day in Bandung
1. On this date, the outside temperature in Bandung is far above average

The first two sentences are literally similar, because they differ in only one word, but semantically, their meaning is opposite. The first and last sentence are very different literally, using completely different words, but their meaning is very similar.