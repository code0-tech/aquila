package tech.code0.data.flow;

import com.google.gson.Gson;
import io.lettuce.core.api.async.RedisAsyncCommands;
import tech.code0.grpc.FlowOuterClass;

import java.util.concurrent.CompletableFuture;

public class FlowService {

    private final RedisAsyncCommands<String, String> asyncCommands;

    public FlowService(RedisAsyncCommands<String, String> asyncCommands) {
        this.asyncCommands = asyncCommands;
    }

    public CompletableFuture<FlowOuterClass.Flow> getFlow(String flow_id) {
        return this.asyncCommands
                .get(flow_id)
                .thenApply(value -> new Gson().fromJson(value, FlowOuterClass.Flow.class))
                .toCompletableFuture();
    }
}