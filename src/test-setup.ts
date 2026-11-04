// jsdom has no canvas. It already answers getContext with null, which the
// waveform code handles, but prints a warning per call that buries the test
// output; this answers the same without the noise.
HTMLCanvasElement.prototype.getContext = (() => null) as typeof HTMLCanvasElement.prototype.getContext;
