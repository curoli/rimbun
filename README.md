# Rimbun

Rimbun is a project to create a new kind of collaborative content creation platform, where differences between versions from different contributors can easily be seen and browsed, with the help of AI.

## Motivation

When you see a sentence on Wikipedia, all you know is that some one brought this sentence into its current form, and since then, no one changed it. You can not see immediately whether there is strong disagreement to it or not.

Sure, you can browse the version history. But that means scanning all changes ever made, and there is no easy way to see, which changes affect this particular sentence.

With git, there is a feature to find out who made the last change to a line, and when it was made, and one could look up the relevant commit and see what was before. But that requires considerable effort, too.

The idea of Rimbun is that as you see the text, you can immediately see where alternative versions exist, and you can browse them and compare them immediately without much effort.

To a degree, of course. When too many versions exist, not all can be shown. Also, a version might have been downvoted enough to be deemed unworthy to be shown, or excluded for other reasons. But where alternative versions exist with substantial community support, these version will absolutely be immediately visible.

To achieve this goal, Rimbun needs to be aware of two different kinds of similarity between versions: textual and semantic.

Textual similarity means, for example, two texts have identical segments. For example, two sentences where only one word differs. Textual similarity is essential when multiple versions are shown to readers for comparison.

Semantic similarity means, two versions have a similar meaning or impact. Semantic similarity can be detected through user feedback or through AI. Semantic similarity is essential when deciding which versions are prioritized for showing to readers.

For example, take these three sentences:

1. Today is a hot day in Bandung
1. Today is a cold day in Bandung
1. On this date, the outside temperature in Bandung is far above average

The first two sentences are textually similar, because they differ in only one word, but semantically, their meaning is opposite. The first and last sentence are very different textually, using completely different words, but their meaning is very similar.

## Semantic organization

Versions are organized according the meaning or impact. Essentially, we form clusters of versions with similar meaning or impact, and pick from each cluster a representative version that is similar to the other versions of the cluster. Subsequently reducing the number of clusters, and therefore the number of representative versions, we can get a ranking of such versions. The one that is most representative of all versions is the main version. A few other versions that correspond to representative versions when there are only few clusters are principal alternatives.

Readers will first see the main version, then the principal alternatives, and if they want to drill deeper, the other versions.

To rank versions, including identifying a main version and principal alternatives, we therefore need a way to measure similarity. The most important tools for these are AI and analysis of user feedback.

AI, more precisely a large language model (LLM), can be used to turn a text into a vector that represents its meaning. Cosine simiarity is considered most suitable to indicate similarity. An open-source locally run model should be more than sufficient and costs much less than using the most powerful models through online services.

Similarity can also be assesssed through user feedback, such as up- and downvotes. Assuming that similar versions get voted on similarly by similar users, we co-assess similarity of versions and users.

## Text organization

Different versions can be created independently, or by editing existing versions, while edits may be big or small. This means two versions can be entirely different, or they could have parts in common. The difference may be a small as using one blank instead of two.

A reader will see a portion of text we call the viewport, which may be the entire text or a part of it. The viewport will display the main version in a way that it can be read continuously just like reading a regular document. The existence of principal alternatives will be displayed prominently, with the option to open them immediately. Other versions are hinted at and can be accessed also, but not as prominently or easily as the principal alternatives.

The way that principal alterntaives are shown depends on how much they align with what appears from the main version in the viewport. More precisely, are some segments of the main version within the viewport identical to segments of the principal versions?

If the principal alternatives have no such segments in common with the main version, then they are simply indicated by notes outside the viewport.

If, on the other hand, substantial portions of the main version within the viewport are identical to portions of a main alternative, then we mark, in the viewport, the portions that are different to indicate that the principal alternatives are different in these particular places.

## Etymology

The name Rimbun is an Indonesian word meaning thick when applied to things that grow, like hair or plants. The name symbolizes the richness of differnet versions that Rimbun can support.