package tech.code0.service;

import io.lettuce.core.RedisFuture;
import io.lettuce.core.api.StatefulRedisConnection;
import io.lettuce.core.api.async.RedisAsyncCommands;
import io.micronaut.test.extensions.junit5.annotation.MicronautTest;
import jakarta.inject.Inject;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.mockito.Mock;
import org.mockito.MockitoAnnotations;
import tech.code0.model.FlowOuterClass;

import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutionException;

import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.eq;
import static org.mockito.Mockito.*;

@MicronautTest
public class FlowServiceTest {

    @Mock
    private StatefulRedisConnection<String, String> connection;

    @Mock
    private RedisAsyncCommands<String, String> commands;

    @Inject
    private FlowService flowService;

    @BeforeEach
    void setUp() {
        MockitoAnnotations.openMocks(this);
        when(connection.async()).thenReturn(commands);
    }

    @Test
    void testUpdateFlowTrue() throws Exception {
        FlowOuterClass.Flow flow = FlowOuterClass.Flow.newBuilder().setFlowId(1).build();

        RedisFuture<String> redisFuture = mock(RedisFuture.class);
        when(commands.set(eq("1:1"), any())).thenReturn(redisFuture);
        when(redisFuture.get()).thenReturn("OK");

        CompletableFuture<Boolean> result = flowService.updateFlow(1L, flow);
        assertTrue(result.join());
    }

    @Test
    void testUpdateFlows_Success() throws ExecutionException, InterruptedException {
        // Arrange
        FlowOuterClass.Flow flow1 = FlowOuterClass.Flow.newBuilder().setFlowId(1).build();
        FlowOuterClass.Flow flow2 = FlowOuterClass.Flow.newBuilder().setFlowId(2).build();
        List<FlowOuterClass.Flow> flows = List.of(flow1, flow2);
        RedisFuture<String> redisFuture = mock(RedisFuture.class);

        when(commands.set(eq("1:1"), any())).thenReturn(redisFuture);
        when(commands.set(eq("1:2"), any())).thenReturn(redisFuture);
        when(redisFuture.get()).thenReturn("OK");

        CompletableFuture<Boolean> result = flowService.updateFlows(1L, flows);
        assertTrue(result.join());
    }

    @Test
    void testDeleteFlow_Success() throws Exception {
        RedisFuture<Long> redisFuture = mock(RedisFuture.class);
        when(commands.del(eq("1:1"))).thenReturn(redisFuture);
        when(redisFuture.get()).thenReturn(1L);

        CompletableFuture<Boolean> result = flowService.deleteFlow(1L, 1L);
        assertTrue(result.join());
    }

    @Test
    void testDeleteFlows_Success() throws Exception {
        FlowOuterClass.Flow flow1 = FlowOuterClass.Flow.newBuilder().setFlowId(1).build();
        FlowOuterClass.Flow flow2 = FlowOuterClass.Flow.newBuilder().setFlowId(2).build();
        List<FlowOuterClass.Flow> flows = List.of(flow1, flow2);

        RedisFuture<Long> redisFuture = mock(RedisFuture.class);

        when(commands.del(any(String[].class))).thenReturn(redisFuture);
        when(redisFuture.get()).thenReturn(2L);

        CompletableFuture<Boolean> result = flowService.deleteFlows(1L, flows);
        assertTrue(result.join());
    }

    @Test
    void testOverWrite_Success() throws ExecutionException, InterruptedException {
        FlowOuterClass.Flow flow1 = FlowOuterClass.Flow.newBuilder().setFlowId(1).build();
        FlowOuterClass.Flow flow2 = FlowOuterClass.Flow.newBuilder().setFlowId(2).build();
        List<FlowOuterClass.Flow> flows = List.of(flow1, flow2);

        RedisFuture<List<String>> redisKeysFuture = mock(RedisFuture.class);
        RedisFuture<Long> redisDelFuture = mock(RedisFuture.class);

        when(commands.keys(any())).thenReturn(redisKeysFuture);
        when(redisKeysFuture.get()).thenReturn(List.of("1:1", "1:2"));
        when(commands.del(any(String[].class))).thenReturn(redisDelFuture);
        when(redisDelFuture.get()).thenReturn(2L);

        CompletableFuture<Boolean> result = flowService.updateFlows(1L, flows);
        assertTrue(result.join());
    }

    @Test
    void testUpdateFlows_EmptyList() {
        List<FlowOuterClass.Flow> emptyList = List.of();
        CompletableFuture<Boolean> result = flowService.updateFlows(1L, emptyList);

        assertTrue(result.join());
    }

}