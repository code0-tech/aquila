package tech.code0.service.checksum;

import com.google.common.util.concurrent.FutureCallback;
import io.grpc.Status;
import org.jetbrains.annotations.NotNull;
import org.slf4j.Logger;
import tech.code0.data.RedisConnection;
import tech.code0.grpc.FlowOuterClass;
import tech.code0.validation.ChecksumValidation;

public class FlowCallback implements FutureCallback<FlowOuterClass.Flow> {

    private final Logger logger;
    private final RedisConnection connection;
    public final FlowOuterClass.Flow currentFlow;

    public FlowCallback(FlowOuterClass.Flow currentFlow, RedisConnection redisConnection, Logger logger) {
        this.logger = logger;
        this.currentFlow = currentFlow;
        this.connection = redisConnection;
    }

    @Override
    public void onSuccess(FlowOuterClass.Flow result) {
        final var isOutdated = ChecksumValidation.hasEqualTime(currentFlow, result);
        if (!isOutdated) return;
        this.connection.getConnection().async().set(STR."flow:\{currentFlow.getFlowId()}", result.toString());
    }

    @Override
    public void onFailure(@NotNull Throwable throwable) {
        final var status = Status.fromThrowable(throwable.getCause());
        this.logger.error(STR."Flow couldn't be recived\{status.getDescription()}", throwable);
    }
}