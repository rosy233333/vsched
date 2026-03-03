# Waker支持修改方案

为了使调度器支持协程`Waker`，需要增加“通过`await Future`阻塞，通过`Waker`唤醒”的情况。因此，需要在这种情况中，处理好任务状态与任务所在队列的维护。

任务状态：目前对协程的操作都会在返回时修改任务状态。（让出或在阻塞协程返回前即唤醒则修改为`Ready`，阻塞在阻塞队列中则修改为`Blocked`）。因此，使用`await`阻塞时协程状态仍为`Running`，且其可用于判断一个判断协程是否使用通过`await Future`阻塞。在这之后，需要在`coroutine_schedule`函数中将其状态改为`Blocked`，并在调用`Waker`唤醒时将状态改为`Ready`。

任务队列：当协程通过`await Future`阻塞，则其不会进行任何队列操作直接返回`coroutine_schedule`。但`coroutine_schedule`要求返回时，已经更新了当前任务和上一任务，且已经把下一任务（新的当前任务）的状态更新。因此，需要在此情况下调用`vsched_apis::resched_f`以补充这一工作。