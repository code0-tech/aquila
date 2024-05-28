package tech.code0.validation;

import lombok.experimental.UtilityClass;
import tech.code0.grpc.FlowOuterClass;

import java.time.LocalDateTime;
import java.time.format.DateTimeFormatter;

@UtilityClass
public class ChecksumValidation {

    public boolean hasEqualTime(FlowOuterClass.Flow usedFlow, FlowOuterClass.Flow accurateFlow) {
        final var formatter = DateTimeFormatter.ISO_LOCAL_DATE_TIME;
        final var usedTime = LocalDateTime.parse(usedFlow.getLastUpdated(), formatter);
        final var accurateTime = LocalDateTime.parse(accurateFlow.getLastUpdated(), formatter);
        return usedTime.isEqual(accurateTime);
    }

}
