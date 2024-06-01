package tech.code0.data;

import com.google.gson.Gson;
import com.google.gson.JsonSyntaxException;
import io.lettuce.core.RedisClient;
import io.lettuce.core.api.StatefulRedisConnection;
import io.lettuce.core.api.async.RedisAsyncCommands;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import lombok.Getter;
import tech.code0.configuration.AquilaConfiguration;
import tech.code0.grpc.FlowOuterClass;

import java.util.Optional;
import java.util.concurrent.CompletableFuture;

public class RedisConnection {

    @Getter
    private final StatefulRedisConnection<String, String> connection;
    @Getter
    private final RedisAsyncCommands<String, String> asyncCommands;
    @Getter
    private final String connectionString;
    @Getter
    private final RedisClient client;

    public final Logger logger;

    public RedisConnection(AquilaConfiguration configuration) {
        this.logger = LoggerFactory.getLogger(RedisConnection.class);

        this.logger.info("Initializing Redis connection");
        this.connectionString = STR."redis://:flows@\{configuration.getRedisHost()}:\{configuration.getRedisPort()}";
        this.client = RedisClient.create(connectionString);
        this.connection = client.connect();
        this.asyncCommands = connection.async();
        this.logger.info("Connected to Redis");
    }

    public void shutdown() {
        this.logger.info("Shutting down RedisConnection");
        this.connection.close();
        this.client.shutdown();
    }

    public CompletableFuture<Optional<FlowOuterClass.Flow>> getFlow(String flowId) {
        final var resultFuture = asyncCommands.get(flowId);
        return resultFuture.thenApply(this::parseFlow).toCompletableFuture();
    }

    private Optional<FlowOuterClass.Flow> parseFlow(String flow) {

        if (flow == null) return Optional.empty();

        try {

            final var currentFlow = new Gson().fromJson(flow, FlowOuterClass.Flow.class);
            return Optional.of(currentFlow);

        } catch (JsonSyntaxException jsonSyntaxException) {

            this.logger.warn(STR."Error parsing flow response with id: \{flow}", jsonSyntaxException);
            return Optional.empty();
        }
    }
}